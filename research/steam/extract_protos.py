"""Extrae los FileDescriptorProto serializados embebidos en un binario (steamclient.so)."""
import re, sys
from google.protobuf import descriptor_pb2

def varint(b, i):
    r = s = 0
    while True:
        c = b[i]; i += 1; r |= (c & 0x7f) << s; s += 7
        if c < 0x80: return r, i

def try_parse(b, start):
    # Un FileDescriptorProto empieza con field 1 (name): 0x0a <len> "<x>.proto"
    # Leemos campos hasta que deje de ser un mensaje válido; probamos longitudes decrecientes.
    i = start; ok_end = None
    try:
        while i < len(b):
            key, j = varint(b, i)
            fn, wt = key >> 3, key & 7
            if not (1 <= fn <= 14) or wt not in (0, 2): break
            if wt == 0: _, j = varint(b, j)
            else:
                ln, j = varint(b, j); j += ln
                if j > len(b): break
            i = j
            fd = descriptor_pb2.FileDescriptorProto()
            try:
                fd.ParseFromString(b[start:i])
                if fd.name.endswith('.proto'): ok_end = i
            except Exception:
                pass
    except IndexError:
        pass
    return ok_end

def main(so, outdir):
    b = open(so, 'rb').read()
    fds = descriptor_pb2.FileDescriptorSet(); seen = set()
    for m in re.finditer(rb'\x0a([\x05-\x7f])([a-z0-9_/.]+\.proto)', b):
        if m.group(1)[0] != len(m.group(2)): continue
        end = try_parse(b, m.start())
        if not end: continue
        fd = descriptor_pb2.FileDescriptorProto(); fd.ParseFromString(b[m.start():end])
        if fd.name in seen or fd.name.startswith('google/'): continue
        seen.add(fd.name); fds.file.append(fd)
    open(f'{outdir}/steamclient.fds.pb', 'wb').write(fds.SerializeToString())
    for fd in fds.file:
        print(fd.name, len(fd.message_type), 'msgs', len(fd.service), 'svcs')
    print('total', len(fds.file))

if __name__ == '__main__':
    main(*sys.argv[1:3])
