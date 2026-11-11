"""Valida un FileDescriptorSet y lo vuelca como archivos .proto legibles."""
import os, sys
from google.protobuf import descriptor_pb2, descriptor_pool
F = descriptor_pb2.FieldDescriptorProto
TYPES = {v: k[5:].lower() for k, v in F.Type.items()}
LABELS = {F.LABEL_OPTIONAL: 'optional', F.LABEL_REQUIRED: 'required', F.LABEL_REPEATED: 'repeated'}

def field(f, ind):
    t = f.type_name.lstrip('.') if f.type_name else TYPES[f.type]
    opts = []
    if f.HasField('default_value'):
        dv = f.default_value
        opts.append(f'default = {dv!r}'.replace("'", '"') if f.type == F.TYPE_STRING else f'default = {dv}')
    if f.options.packed: opts.append('packed = true')
    o = f' [{", ".join(opts)}]' if opts else ''
    return f'{ind}{LABELS[f.label]} {t} {f.name} = {f.number}{o};'

def enum(e, ind):
    out = [f'{ind}enum {e.name} {{']
    out += [f'{ind}  {v.name} = {v.number};' for v in e.value]
    return out + [f'{ind}}}']

def msg(m, ind=''):
    out = [f'{ind}message {m.name} {{']
    for e in m.enum_type: out += enum(e, ind + '  ')
    for n in m.nested_type:
        if n.options.map_entry: continue
        out += msg(n, ind + '  ')
    oneofs = {}
    for f in m.field:
        if f.HasField('oneof_index'): oneofs.setdefault(f.oneof_index, []).append(f)
        else: out.append(field(f, ind + '  '))
    for i, fs in oneofs.items():
        out.append(f'{ind}  oneof {m.oneof_decl[i].name} {{')
        out += [field(f, ind + '    ').replace('optional ', '') for f in fs]
        out.append(f'{ind}  }}')
    return out + [f'{ind}}}']

def main(fds_path, outdir):
    fds = descriptor_pb2.FileDescriptorSet(); fds.ParseFromString(open(fds_path, 'rb').read())
    pool = descriptor_pool.DescriptorPool()
    d = descriptor_pb2.FileDescriptorProto(); descriptor_pb2.DESCRIPTOR.CopyToProto(d); pool.Add(d)
    pending = list(fds.file); added = set(); missing = set()
    while pending:  # añadir en orden de dependencias
        prog = False
        for fd in list(pending):
            deps = [d for d in fd.dependency if not d.startswith('google/')]
            if all(d in added for d in deps):
                pool.Add(fd); added.add(fd.name); pending.remove(fd); prog = True
        if not prog:
            for fd in pending: missing |= {d for d in fd.dependency if d not in added and not d.startswith('google/')}
            break
    print('validados', len(added), 'de', len(fds.file), '| deps faltantes:', sorted(missing))
    for fd in fds.file:
        out = [f'// Extraído de steamclient.so', f'syntax = "{fd.syntax or "proto2"}";']
        if fd.package: out.append(f'package {fd.package};')
        out += [f'import "{d}";' for d in fd.dependency] + ['']
        for e in fd.enum_type: out += enum(e, '') + ['']
        for m in fd.message_type: out += msg(m) + ['']
        for s in fd.service:
            out.append(f'service {s.name} {{')
            out += [f'  rpc {r.name} ({r.input_type.lstrip(".")}) returns ({r.output_type.lstrip(".")});' for r in s.method]
            out += ['}', '']
        p = os.path.join(outdir, fd.name); os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p, 'w').write('\n'.join(out))

if __name__ == '__main__':
    main(*sys.argv[1:3])
