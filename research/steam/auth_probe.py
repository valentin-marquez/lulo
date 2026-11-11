"""Sonda del servicio Authentication sin credenciales reales.

Usa una cuenta inventada (no existe) para validar transporte, formato RSA y respuestas de error,
y el flujo QR hasta el polling (sin aprobar). Uso: python3 -I auth_probe.py
"""
import base64, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cm_client as c

NONAUTHED = 9804  # k_EMsgServiceMethodCallFromClientNonAuthed
c.EMSG_NAME[NONAUTHED] = 'ServiceMethodCallFromClientNonAuthed'


def call(cm, method, req, resp_type, emsg):
    """Como CM.call pero con EMsg elegible y sin abortar en eresult != 1."""
    cm.job += 1
    cm.send(emsg, req, target_job_name=f'{method}#1', jobid_source=cm.job)
    hdr, body = cm.wait(c.EMSG['ServiceMethodResponse'], cm.job)
    r = c.M(resp_type); r.ParseFromString(body)
    return hdr.eresult, hdr.error_message, r


def rsa_pkcs1v15(mod_hex, exp_hex, msg):
    """RSA PKCS#1 v1.5 (tipo 2) con stdlib: EM = 00 02 PS(!=0) 00 M."""
    n, e = int(mod_hex, 16), int(exp_hex, 16)
    k = (n.bit_length() + 7) // 8
    ps = bytes(b % 255 + 1 for b in os.urandom(k - 3 - len(msg)))
    em = b'\x00\x02' + ps + b'\x00' + msg
    return base64.b64encode(pow(int.from_bytes(em, 'big'), e, n).to_bytes(k, 'big')).decode()


def device(r):
    r.device_friendly_name = 'lulo-re-probe'
    r.platform_type = 1  # k_EAuthTokenPlatformType_SteamClient


def main():
    fake = 'lulo_re_probe_' + os.urandom(4).hex()
    print(f'cuenta inventada: {fake}')

    # --- A. Sin ClientLogon: ¿se puede hablar con Authentication por 9804? ---
    cm = c.CM(c.WS(*c.cm_endpoint())); cm.steamid = 0  # antes del logon: steamid 0
    hello = c.M('CMsgClientHello'); hello.protocol_version = c.PROTOCOL_VERSION
    cm.send(c.EMSG['ClientHello'], hello)
    rq = c.M('CAuthentication_GetPasswordRSAPublicKey_Request'); rq.account_name = fake
    er, msg, rsa = call(cm, 'Authentication.GetPasswordRSAPublicKey', rq,
                        'CAuthentication_GetPasswordRSAPublicKey_Response', NONAUTHED)
    print(f'<- [9804, sin logon] GetPasswordRSAPublicKey eresult={er} {msg!r} '
          f'mod={len(rsa.publickey_mod)*4} bits exp={rsa.publickey_exp} timestamp={rsa.timestamp}')

    if er == 1:
        rb = c.M('CAuthentication_BeginAuthSessionViaCredentials_Request'); device(rb)
        rb.account_name = fake
        rb.encrypted_password = rsa_pkcs1v15(rsa.publickey_mod, rsa.publickey_exp, b'contrasena-falsa')
        rb.encryption_timestamp = rsa.timestamp
        rb.remember_login = True; rb.persistence = 1
        rb.device_details.device_friendly_name = 'lulo-re-probe'; rb.device_details.platform_type = 1
        rb.device_details.os_type = 20
        er, msg, br = call(cm, 'Authentication.BeginAuthSessionViaCredentials', rb,
                           'CAuthentication_BeginAuthSessionViaCredentials_Response', NONAUTHED)
        print(f'<- [9804, sin logon] BeginAuthSessionViaCredentials eresult={er} {msg!r} '
              f'ext={br.extended_error_message!r}')

    # --- B. QR + polling (sin aprobar) ---
    rq = c.M('CAuthentication_BeginAuthSessionViaQR_Request'); device(rq)
    rq.device_details.device_friendly_name = 'lulo-re-probe'; rq.device_details.platform_type = 1
    er, msg, qr = call(cm, 'Authentication.BeginAuthSessionViaQR', rq,
                       'CAuthentication_BeginAuthSessionViaQR_Response', NONAUTHED)
    print(f'<- [9804] BeginAuthSessionViaQR eresult={er} client_id={qr.client_id} '
          f'request_id={len(qr.request_id)}B interval={qr.interval} confirmaciones='
          f'{[x.confirmation_type for x in qr.allowed_confirmations]} url={qr.challenge_url[:24]}...')
    for i in range(2):
        time.sleep(qr.interval or 5)
        rp = c.M('CAuthentication_PollAuthSessionStatus_Request'); rp.client_id = qr.client_id
        rp.request_id = qr.request_id
        er, msg, pr = call(cm, 'Authentication.PollAuthSessionStatus', rp,
                           'CAuthentication_PollAuthSessionStatus_Response', NONAUTHED)
        print(f'<- [9804] PollAuthSessionStatus #{i+1} eresult={er} campos={[f.name for f, _ in pr.ListFields()]} '
              f'new_challenge_url={"sí" if pr.new_challenge_url else "no"}')

    # --- C. GenerateAccessTokenForApp con token basura ---
    rg = c.M('CAuthentication_AccessToken_GenerateForApp_Request')
    # Token falso armado aquí (cabecera JWT válida, payload vacío, firma basura): no es un secreto.
    b64 = lambda raw: base64.urlsafe_b64encode(raw).rstrip(b'=').decode()
    rg.refresh_token = '.'.join([b64(b'{"typ":"JWT","alg":"EdDSA"}'), b64(b'{}'), 'AAAA'])
    rg.steamid = 76561197960265728  # SteamID base, sin cuenta
    rg.renewal_type = 1
    er, msg, gr = call(cm, 'Authentication.GenerateAccessTokenForApp', rg,
                       'CAuthentication_AccessToken_GenerateForApp_Response', NONAUTHED)
    print(f'<- [9804] GenerateAccessTokenForApp (token basura) eresult={er} {msg!r}')


if __name__ == '__main__':
    main()
