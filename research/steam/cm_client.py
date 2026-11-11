"""Cliente CM mínimo (solo stdlib + protobuf) para validar el protocolo contra el servidor real.

Uso:
  python3 -I cm_client.py workshop <publishedfileid> <dir_salida>   # login anónimo + descarga de un mod de PZ
Cada paso imprime lo que envía/recibe, para documentar el protocolo.
"""
import base64, gzip, json, os, socket, ssl, struct, sys, urllib.request, zlib
from google.protobuf import descriptor_pb2, descriptor_pool, message_factory

HERE = os.path.dirname(os.path.abspath(__file__))
PROTO_MASK = 0x80000000
EMSG = dict(Multi=1, ServiceMethodResponse=147, ServiceMethodCallFromClient=151, ClientLogOnResponse=751,
            ClientGetDepotDecryptionKey=5438, ClientGetDepotDecryptionKeyResponse=5439, ClientLogon=5514,
            ClientHello=9805)
EMSG_NAME = {v: k for k, v in EMSG.items()}
ANON_STEAMID = (1 << 56) | (10 << 52)  # universo Public, tipo AnonUser
PROTOCOL_VERSION = 65580


# ---------- protobufs extraídos de steamclient.so ----------
def load_pool():
    fds = descriptor_pb2.FileDescriptorSet(); fds.ParseFromString(open(f'{HERE}/protos/steamclient.fds.pb', 'rb').read())
    pool = descriptor_pool.DescriptorPool()
    d = descriptor_pb2.FileDescriptorProto(); descriptor_pb2.DESCRIPTOR.CopyToProto(d); pool.Add(d)
    pending, added = list(fds.file), set()
    while pending:
        for fd in list(pending):
            if all(x in added or x.startswith('google/') for x in fd.dependency):
                pool.Add(fd); added.add(fd.name); pending.remove(fd)
    return pool

POOL = load_pool()
def M(name): return message_factory.GetMessageClass(POOL.FindMessageTypeByName(name))()


# ---------- WebSocket mínimo (RFC 6455, cliente) ----------
class WS:
    def __init__(self, host, port):
        raw = socket.create_connection((host, port), timeout=15)
        self.s = ssl.create_default_context().wrap_socket(raw, server_hostname=host)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall((f'GET /cmsocket/ HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\n'
                        f'Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        resp = b''
        while b'\r\n\r\n' not in resp: resp += self.s.recv(1)
        assert b' 101 ' in resp.split(b'\r\n')[0], resp

    def _read(self, n):
        b = b''
        while len(b) < n:
            c = self.s.recv(n - len(b))
            if not c: raise EOFError('ws cerrado')
            b += c
        return b

    def send(self, payload):
        hdr = bytearray([0x82])  # FIN + binario
        n = len(payload)
        if n < 126: hdr.append(0x80 | n)
        elif n < 65536: hdr += bytes([0x80 | 126]) + struct.pack('>H', n)
        else: hdr += bytes([0x80 | 127]) + struct.pack('>Q', n)
        mask = os.urandom(4)
        self.s.sendall(bytes(hdr) + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload)))

    def recv(self):
        data = b''
        while True:
            b0, b1 = self._read(2)
            n = b1 & 0x7f
            if n == 126: n = struct.unpack('>H', self._read(2))[0]
            elif n == 127: n = struct.unpack('>Q', self._read(8))[0]
            chunk = self._read(n); op = b0 & 0x0f
            if op == 8: raise EOFError(f'close {chunk[:2].hex()} {chunk[2:]}')
            if op == 9: continue  # ping (ignoramos pong en esta prueba)
            data += chunk
            if b0 & 0x80: return data


# ---------- Encuadre de mensajes Steam ----------
class CM:
    def __init__(self, ws):
        self.ws, self.steamid, self.session, self.job, self.queue = ws, ANON_STEAMID, 0, 0, []

    def send(self, emsg, body, **hdr_fields):
        hdr = M('CMsgProtoBufHeader'); hdr.steamid = self.steamid; hdr.client_sessionid = self.session
        for k, v in hdr_fields.items(): setattr(hdr, k, v)
        h = hdr.SerializeToString()
        self.ws.send(struct.pack('<II', emsg | PROTO_MASK, len(h)) + h + body.SerializeToString())
        print(f'-> {EMSG_NAME.get(emsg, emsg)} {hdr_fields or ""}')

    def _split(self, pkt):
        raw, = struct.unpack_from('<I', pkt)
        emsg = raw & ~PROTO_MASK
        assert raw & PROTO_MASK, f'mensaje no protobuf emsg={emsg}'
        hl, = struct.unpack_from('<I', pkt, 4)
        hdr = M('CMsgProtoBufHeader'); hdr.ParseFromString(pkt[8:8 + hl])
        return emsg, hdr, pkt[8 + hl:]

    def _expand(self, pkt):
        emsg, hdr, body = self._split(pkt)
        if emsg != EMSG['Multi']: return [(emsg, hdr, body)]
        multi = M('CMsgMulti'); multi.ParseFromString(body)
        data = gzip.decompress(multi.message_body) if multi.size_unzipped else multi.message_body
        out, i = [], 0
        while i < len(data):
            n, = struct.unpack_from('<I', data, i); out += self._expand(data[i + 4:i + 4 + n]); i += 4 + n
        return out

    def wait(self, want_emsg=None, job=None):
        while True:
            if not self.queue: self.queue = self._expand(self.ws.recv())
            emsg, hdr, body = self.queue.pop(0)
            if (want_emsg is None or emsg == want_emsg) and (job is None or hdr.jobid_target == job):
                return hdr, body
            print(f'   (ignorado {EMSG_NAME.get(emsg, emsg)})')

    def call(self, method, req, resp_type):
        """Llamada a servicio unificado: EMsg 151 con target_job_name 'Servicio.Metodo#1'."""
        self.job += 1
        self.send(EMSG['ServiceMethodCallFromClient'], req, target_job_name=f'{method}#1', jobid_source=self.job)
        hdr, body = self.wait(EMSG['ServiceMethodResponse'], self.job)
        assert hdr.eresult == 1, f'{method} eresult={hdr.eresult} {hdr.error_message}'
        r = M(resp_type); r.ParseFromString(body); return r


def cm_endpoint():
    url = 'https://api.steampowered.com/ISteamDirectory/GetCMListForConnect/v1/?cellid=0'
    servers = json.load(urllib.request.urlopen(url))['response']['serverlist']
    ws = [s for s in servers if s['type'] == 'websockets']
    print(f'CM list: {len(servers)} servidores, {len(ws)} websocket -> {ws[0]["endpoint"]}')
    h, p = ws[0]['endpoint'].rsplit(':', 1); return h, int(p)


def login_anonymous():
    cm = CM(WS(*cm_endpoint()))
    hello = M('CMsgClientHello'); hello.protocol_version = PROTOCOL_VERSION
    cm.send(EMSG['ClientHello'], hello)
    logon = M('CMsgClientLogon'); logon.protocol_version = PROTOCOL_VERSION
    logon.client_os_type = 20  # hipótesis: valor Linux; validar si el servidor lo exige
    logon.cell_id = 0
    cm.send(EMSG['ClientLogon'], logon)
    hdr, body = cm.wait(EMSG['ClientLogOnResponse'])
    r = M('CMsgClientLogonResponse'); r.ParseFromString(body)
    print(f'<- LogOnResponse eresult={r.eresult} steamid={hdr.steamid:#x} session={hdr.client_sessionid} '
          f'heartbeat={r.heartbeat_seconds}s cell={r.cell_id}')
    assert r.eresult == 1
    cm.steamid, cm.session = hdr.steamid, hdr.client_sessionid
    return cm, r.cell_id


def workshop(pubid, outdir):
    sys.path.insert(0, HERE); import verify_depot as vd
    APP = DEPOT = 108600
    cm, cell = login_anonymous()

    # 1. Detalles del item (servicio PublishedFile)
    req = M('CPublishedFile_GetDetails_Request'); req.publishedfileids.append(int(pubid))
    det = cm.call('PublishedFile.GetDetails', req, 'CPublishedFile_GetDetails_Response').publishedfiledetails[0]
    print(f'<- GetDetails "{det.title}" hcontent_file={det.hcontent_file} size={det.file_size} file_url={det.file_url!r}')

    # 2. Clave del depot (mensaje clásico, no servicio)
    cm.job += 1
    k = M('CMsgClientGetDepotDecryptionKey'); k.depot_id = DEPOT; k.app_id = APP
    cm.send(EMSG['ClientGetDepotDecryptionKey'], k, jobid_source=cm.job)
    _, body = cm.wait(EMSG['ClientGetDepotDecryptionKeyResponse'], cm.job)
    kr = M('CMsgClientGetDepotDecryptionKeyResponse'); kr.ParseFromString(body)
    print(f'<- DepotDecryptionKey eresult={kr.eresult} depot={kr.depot_id} key={kr.depot_encryption_key[:4].hex()}...')
    assert kr.eresult == 1

    # 3. Servidores de contenido
    sreq = M('CContentServerDirectory_GetServersForSteamPipe_Request'); sreq.cell_id = cell
    servers = cm.call('ContentServerDirectory.GetServersForSteamPipe', sreq,
                      'CContentServerDirectory_GetServersForSteamPipe_Response').servers
    host = next(s.host for s in servers if s.https_support in ('mandatory', 'optional'))
    print(f'<- {len(servers)} servidores CDN, uso {host}')

    # 4. Código de request del manifest
    mreq = M('CContentServerDirectory_GetManifestRequestCode_Request')
    mreq.app_id, mreq.depot_id, mreq.manifest_id, mreq.app_branch = APP, DEPOT, det.hcontent_file, 'public'
    code = cm.call('ContentServerDirectory.GetManifestRequestCode', mreq,
                   'CContentServerDirectory_GetManifestRequestCode_Response').manifest_request_code
    print(f'<- manifest_request_code={code}')

    # 5. Manifest + chunks (pipeline ya verificado en verify_depot.py)
    os.makedirs(outdir, exist_ok=True); mpath = os.path.join(outdir, 'manifest.zip')
    url = f'https://{host}/depot/{DEPOT}/manifest/{det.hcontent_file}/5/{code}'
    open(mpath, 'wb').write(urllib.request.urlopen(url).read()); print(f'GET {url} OK')
    vd.download(mpath, kr.depot_encryption_key, host, DEPOT, os.path.join(outdir, 'files'))


def parse_vdf(text):
    """KeyValues en texto: "clave" "valor" | "clave" { ... }. Suficiente para appinfo."""
    import re
    toks = re.findall(r'"((?:[^"\\]|\\.)*)"|([{}])', text)
    def block(i):
        d = {}
        while i < len(toks):
            s, br = toks[i]
            if br == '}': return d, i + 1
            key = s; s2, br2 = toks[i + 1]
            if br2 == '{': d[key], i = block(i + 2)
            else: d[key] = s2; i += 2
        return d, i
    return block(0)[0]


def depot_key(cm, app, depot):
    cm.job += 1
    k = M('CMsgClientGetDepotDecryptionKey'); k.depot_id = depot; k.app_id = app
    cm.send(EMSG['ClientGetDepotDecryptionKey'], k, jobid_source=cm.job)
    _, body = cm.wait(EMSG['ClientGetDepotDecryptionKeyResponse'], cm.job)
    r = M('CMsgClientGetDepotDecryptionKeyResponse'); r.ParseFromString(body); return r


def product_info(cm, app):
    EMSG.update(ClientPICSProductInfoRequest=8903, ClientPICSProductInfoResponse=8904,
                ClientPICSAccessTokenRequest=8905, ClientPICSAccessTokenResponse=8906)
    EMSG_NAME.update({v: k for k, v in EMSG.items()})
    cm.job += 1
    t = M('CMsgClientPICSAccessTokenRequest'); t.appids.append(app)
    cm.send(EMSG['ClientPICSAccessTokenRequest'], t, jobid_source=cm.job)
    _, body = cm.wait(EMSG['ClientPICSAccessTokenResponse'], cm.job)
    tr = M('CMsgClientPICSAccessTokenResponse'); tr.ParseFromString(body)
    token = next((x.access_token for x in tr.app_access_tokens if x.appid == app), 0)
    print(f'<- AccessToken app {app}: {"token" if token else "sin token"} (denegados {list(tr.app_denied_tokens)})')

    cm.job += 1
    req = M('CMsgClientPICSProductInfoRequest'); a = req.apps.add(); a.appid = app; a.access_token = token
    req.single_response = True
    cm.send(EMSG['ClientPICSProductInfoRequest'], req, jobid_source=cm.job)
    while True:
        _, body = cm.wait(EMSG['ClientPICSProductInfoResponse'], cm.job)
        r = M('CMsgClientPICSProductInfoResponse'); r.ParseFromString(body)
        if r.apps: break
        assert r.response_pending, f'sin apps; unknown={list(r.unknown_appids)}'
    ai = r.apps[0]
    if ai.buffer: raw, src = ai.buffer, 'inline'
    else:  # contenido grande: HTTP (formato '/appinfo/%u/sha/%s.txt.gz' de steamclient.so)
        url = f'http://{r.http_host}/appinfo/{app}/sha/{ai.sha.hex()}.txt.gz'
        raw, src = gzip.decompress(urllib.request.urlopen(url).read()), url
    print(f'<- ProductInfo app {app} change={ai.change_number} missing_token={ai.missing_token} via {src}')
    return parse_vdf(raw.rstrip(b'\0').decode('utf-8', 'replace'))['appinfo']


def appinfo(*apps):
    cm, _ = login_anonymous()
    for app in map(int, apps):
        info = product_info(cm, app)
        depots = info.get('depots', {})
        print(f'\n=== {app} {info.get("common", {}).get("name")} ===')
        print('ramas:')
        for name, b in depots.get('branches', {}).items():
            flags = ' (con contraseña)' if b.get('pwdrequired') == '1' else ''
            print(f'  {name:20} build {b.get("buildid"):>9}  {b.get("description", "")}{flags}')
        print('depots:')
        for did, d in depots.items():
            if not did.isdigit(): continue
            cfg = d.get('config', {}); man = d.get('manifests', {})
            pub = man.get('public', {}); gid = pub.get('gid') if isinstance(pub, dict) else pub
            kr = depot_key(cm, app, int(did))
            print(f'  {did} os={cfg.get("oslist", "-"):14} public={gid}  ramas_con_manifest={list(man)}'
                  f'  clave_anónimo={"SÍ" if kr.eresult == 1 else f"NO (eresult {kr.eresult})"}')
    open(f'{HERE}/runs/appinfo_{"_".join(apps)}.json', 'w').write(json.dumps(info, indent=1))


if __name__ == '__main__':
    {'workshop': workshop, 'appinfo': appinfo}[sys.argv[1]](*sys.argv[2:])
