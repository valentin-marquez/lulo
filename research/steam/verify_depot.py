"""Verifica a mano el pipeline de contenido de Steam (manifest -> chunks -> archivo).

Uso: python3 -I verify_depot.py <manifest.bin(zip)> <depot_key_hex> <cdn_host> <depot_id> <dir_instalado_por_steamcmd>
Requiere: openssl en PATH. No usa SteamCMD.
"""
import base64, hashlib, io, lzma, os, struct, subprocess, sys, urllib.request, zipfile, zlib
from google.protobuf import descriptor_pb2, descriptor_pool, message_factory

FDS = os.path.join(os.path.dirname(__file__), 'protos/steamclient.fds.pb')
MAGIC_PAYLOAD, MAGIC_META, MAGIC_SIG, MAGIC_END = 0x71F617D0, 0x1F4812BE, 0x1B81B817, 0x32C415AB


def aes(mode, key, data, iv=None):
    cmd = ['openssl', 'enc', '-d', f'-aes-256-{mode}', '-nopad', '-K', key.hex()]
    if iv: cmd += ['-iv', iv.hex()]
    return subprocess.run(cmd, input=data, capture_output=True, check=True).stdout


def symmetric_decrypt(key, data):
    """Esquema Steam: 16 bytes de IV cifrados con AES-ECB, luego AES-CBC con PKCS7."""
    iv = aes('ecb', key, data[:16])
    out = aes('cbc', key, data[16:], iv)
    return out[:-out[-1]]


def decompress(buf):
    if buf[:2] == b'VZ':  # VZip: 'VZ' 'a' u32 crc/timestamp | 5 bytes props LZMA | datos | u32 crc u32 size 'zv'
        assert buf[2:3] == b'a' and buf[-2:] == b'zv', buf[:3]
        props, data = buf[7:12], buf[12:-10]
        size = struct.unpack('<I', buf[-6:-2])[0]
        filt = lzma._decode_filter_properties(lzma.FILTER_LZMA1, props)
        return lzma.LZMADecompressor(lzma.FORMAT_RAW, filters=[filt]).decompress(data)[:size], 'vzip-lzma'
    if buf[:4] == b'VSZa':  # 'VSZa' u32 crc | frame zstd | u32 crc u32 size u32 0 'zsv'  (pie de 15 bytes)
        assert buf[-3:] == b'zsv' and buf[4:8] == buf[-15:-11], 'framing VSZa'
        size = struct.unpack('<I', buf[-11:-7])[0]
        out = subprocess.run(['zstd', '-dc'], input=buf[8:-15], capture_output=True, check=True).stdout
        assert len(out) == size and struct.pack('<I', zlib.crc32(out)) == buf[4:8], 'crc/size VSZa'
        return out, 'vsza-zstd'
    if buf[:2] == b'PK':
        z = zipfile.ZipFile(io.BytesIO(buf)); return z.read(z.namelist()[0]), 'zip'
    raise SystemExit(f'compresión desconocida: {buf[:8].hex()}')


def files(manifest_zip, key, host, depot):
    """Genera (nombre, bytes|None si es directorio, info) reconstruyendo cada archivo desde el CDN."""
    fds = descriptor_pb2.FileDescriptorSet(); fds.ParseFromString(open(FDS, 'rb').read())
    pool = descriptor_pool.DescriptorPool()
    pool.Add([f for f in fds.file if f.name == 'content_manifest.proto'][0])
    cls = lambda n: message_factory.GetMessageClass(pool.FindMessageTypeByName(n))

    z = zipfile.ZipFile(manifest_zip).read('z'); i = 0; payload = meta = None
    while True:
        magic, = struct.unpack_from('<I', z, i)
        if magic == MAGIC_END: break
        ln, = struct.unpack_from('<I', z, i + 4); body = z[i + 8:i + 8 + ln]; i += 8 + ln
        if magic == MAGIC_PAYLOAD: payload = cls('ContentManifestPayload')(); payload.ParseFromString(body)
        elif magic == MAGIC_META: meta = cls('ContentManifestMetadata')(); meta.ParseFromString(body)
    print(f'manifest {meta.gid_manifest}: {len(payload.mappings)} entradas, cifrado={meta.filenames_encrypted}')

    for m in payload.mappings:
        name = m.filename
        if meta.filenames_encrypted:
            name = symmetric_decrypt(key, base64.b64decode(name)).rstrip(b'\0').decode()
        name = name.replace('\\', '/')
        if m.flags & 64:  # 64 = directorio
            yield name, None, 'dir'; continue
        data = bytearray(m.size); kind = 'vacío'
        for c in sorted(m.chunks, key=lambda c: c.offset):
            url = f'https://{host}/depot/{depot}/chunk/{c.sha.hex()}'
            raw = urllib.request.urlopen(url).read()
            assert len(raw) == c.cb_compressed, (len(raw), c.cb_compressed)
            plain, kind = decompress(symmetric_decrypt(key, raw))
            # El campo 'crc' del manifest es Adler-32 con semilla 0 (no la estándar 1).
            assert len(plain) == c.cb_original and zlib.adler32(plain, 0) == c.crc, 'adler/size chunk'
            assert hashlib.sha1(plain).digest() == c.sha, 'sha chunk'
            data[c.offset:c.offset + len(plain)] = plain
        if m.size: assert hashlib.sha1(data).digest() == m.sha_content, 'sha archivo'
        yield name, bytes(data), f'{m.size} B, {len(m.chunks)} chunk, {kind}'


def safe_join(outdir, name):
    """Refuses manifest paths that would land outside outdir (absolute, drive letters, '..')."""
    root = os.path.realpath(outdir)
    real = os.path.realpath(os.path.join(root, name))
    if os.path.isabs(name) or ':' in name or os.path.commonpath([real, root]) != root:
        raise SystemExit(f'ruta insegura en el manifest: {name!r}')
    return real


def download(manifest_zip, key, host, depot, outdir):
    n = 0
    for name, data, info in files(manifest_zip, key, host, depot):
        p = safe_join(outdir, name)
        if data is None: os.makedirs(p, exist_ok=True); continue
        os.makedirs(os.path.dirname(p), exist_ok=True); open(p, 'wb').write(data); n += 1
        print(f'  escrito {name} ({info})')
    print(f'{n} archivos en {outdir}')


def main(manifest_zip, key_hex, host, depot, installed):
    ok = bad = 0
    for name, data, info in files(manifest_zip, bytes.fromhex(key_hex), host, depot):
        if data is None: continue
        same = open(safe_join(installed, name), 'rb').read() == data; ok += same; bad += not same
        print(f'  {"OK " if same else "DIF"} {name} ({info})')
    print(f'resultado: {ok} idénticos a SteamCMD, {bad} distintos')


if __name__ == '__main__':
    main(*sys.argv[1:6])
