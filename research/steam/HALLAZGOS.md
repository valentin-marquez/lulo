# Steam nativo — hallazgos de ingeniería inversa

Build analizado: SteamCMD `1792617903` (steamclient.so / steamconsole.so de 2026-10).
Leyenda: **[verificado]** = comprobado con bytes reales; **[observado]** = visto en logs de una ejecución real; **[pendiente]** = falta confirmar.

## 1. Esquemas
- `protos/` — 109 `.proto` extraídos de `steamclient.so` (`extract_protos.py`, `render_protos.py`). **[verificado]**: el set cierra sin deps faltantes.
- `protos/steamclient.fds.pb` → `prost_build::Config::compile_fds()` en Rust.
- EMsg relevantes (`enums_clientserver.proto`): Multi=1, ServiceMethodResponse=147, ServiceMethodCallFromClient=151, ClientHeartBeat=703, ClientLogOnResponse=751, ChannelEncrypt*=1303-1305, ClientGetDepotDecryptionKey=5438/5439, ClientLogon=5514, ClientPICSProductInfo=8903/8904, ClientHello=9805.

## 2. Conexión y login  **[observado]**
1. `GET ISteamDirectory/GetCMListForConnect/?cellid=0&qoslevel=3` → CMs Netfilter (TCP) y WebSocket.
2. Ping a varios, conecta por **WebSocket a `cmpN-<dc>.steamserver.net:443`** (sin handshake ChannelEncrypt: va sobre TLS).
3. `ClientLogon` anónimo → `ClientLogOnResponse OK`, SteamID `[a:1:N]` (cuenta anónima, universo 1).
4. Login de usuario (pendiente de analizar): servicio `Authentication` (GetPasswordRSAPublicKey → BeginAuthSessionViaCredentials → UpdateAuthSessionWithSteamGuardCode → PollAuthSessionStatus → refresh token → ClientLogon con token).

## 3. App info (PICS)  **[observado]**
- PICS por CM; el contenido grande de appinfo se baja **por HTTP** ("Downloaded 1 apps via HTTP").
- Pide access tokens por app (anónimo: la mayoría denegados, no hace falta para 380870/1007).

## 4. Contenido (CDN)  **[verificado]** con el mod 3832274571
1. `ContentServerDirectory.GetServersForSteamPipe` → hosts `cacheN-<dc>.steamcontent.com` (tipo SteamCache, HTTPS, HTTP/2).
2. Manifest: `GET https://<host>/depot/<depot>/manifest/<gid>/5/<request_code>`
   - `request_code` viene de `ContentServerDirectory.GetManifestRequestCode` y **expira** (minutos). [observado/pendiente: duración exacta]
   - Respuesta `application/x-steam-manifest`: ZIP con una entrada `z`.
   - `z` = secuencia `[u32 magic LE][u32 len][protobuf]`: Payload `0x71F617D0`, Metadata `0x1F4812BE`, Signature `0x1B81B817`, fin `0x32C415AB` (sin len).
   - Mapping `flags & 64` = directorio.
3. Si `filenames_encrypted`: nombre = base64 → descifrado simétrico Steam con la clave del depot → recortar `\0`. Separador `\`.
4. Chunk: `GET https://<host>/depot/<depot>/chunk/<sha1 hex>` → `cb_compressed` bytes.
5. **Descifrado simétrico Steam**: primeros 16 bytes = IV cifrado con AES-256-ECB; resto AES-256-CBC con ese IV, padding PKCS7.
6. Descompresión según cabecera:
   - `VSZa` (zstd, **lo que usan los chunks actuales**): `"VSZa" | crc32 u32 | frame zstd | crc32 u32 | size u32 | u32 0 | "zsv"` (pie 15 B). crc32 = del contenido plano.
   - `VZa` (LZMA1 raw): `"VZa" | u32 | props 5B | datos | crc u32 | size u32 | "zv"`. **[verificado]** con depot 380873 rama `legacy41` (B41 usa LZMA; B42/workshop actuales usan zstd → hay que soportar ambos).
   - `PK` (zip) en depots antiguos. [pendiente]
7. Verificación: `chunk.crc` del manifest = **Adler-32 con semilla 0**; `chunk.sha` = SHA1 del chunk plano; `mapping.sha_content` = SHA1 del archivo completo.
- Clave del depot: `ClientGetDepotDecryptionKey` (EMsg 5438). SteamCMD la cachea en `config/config.vdf` → `depots/<id>/DecryptionKey`.

## 5. Workshop (PZ)  **[verificado]**
- Funciona **anónimo** para 108600.
- `PublishedFile.GetDetails` (o la web pública `ISteamRemoteStorage/GetPublishedFileDetails`) → `hcontent_file` = **gid del manifest en el depot 108600**. Si `file_url` no está vacío es un item legacy (descarga HTTP directa).
- Después, idéntico a §4 con depot = 108600.
- SteamCMD: descarga en `steamapps/workshop/downloads/108600`, commit a `.../content/108600/<id>`, estado en `appworkshop_108600.acf`.

## 6. Estados de app_update (para la UI)  [del análisis de steamconsole.so]
Bits: 0x1 running, 0x2 reconfiguring, 0x4 verifying install, 0x10 preallocating, 0x20 downloading, 0x40 staging, 0x80 verifying update, 0x100 committing, 0x200 finalizing, 0x400 stopping. App instalada = estado 4.
Con implementación nativa estos estados son nuestros; sirven solo como referencia de fases.

## 7. Protocolo CM por WebSocket  **[verificado]** con `cm_client.py` (descarga del mod idéntica a SteamCMD, sin SteamCMD)
- Endpoint: `wss://<endpoint de GetCMListForConnect>/cmsocket/` (string `https://%s/cmsocket/` en steamclient.so). Sirven tanto :443 como :27019.
- **1 frame WebSocket binario = 1 paquete Steam** (sin cabecera `VT01`, sin ChannelEncrypt; TLS hace el cifrado).
- Paquete protobuf: `u32 LE (emsg | 0x80000000) | u32 LE len_header | CMsgProtoBufHeader | cuerpo`.
- Cabecera saliente: `steamid` + `client_sessionid` (0 antes del login; después los que trae la cabecera de `ClientLogOnResponse`).
- `Multi` (1): `CMsgMulti{size_unzipped, message_body}`; si `size_unzipped>0`, gzip. Dentro: `[u32 len][paquete]` repetido (recursivo).
- Secuencia anónima mínima aceptada por el servidor:
  1. `ClientHello` (9805) `{protocol_version: 65580}`
  2. `ClientLogon` (5514) `{protocol_version: 65580, client_os_type: 20, cell_id: 0}` con steamid `0x01A0000000000000` (AnonUser).
  3. Llega `ClientServersAvailable` (5501, se ignora) y `ClientLogOnResponse` (751): eresult 1, `heartbeat_seconds` = 9, `cell_id`.
- Servicio unificado: EMsg 151 con cabecera `target_job_name="Servicio.Metodo#1"`, `jobid_source=N`; respuesta EMsg 147 con `jobid_target=N`, `eresult` en la cabecera.
  - Verificados: `PublishedFile.GetDetails`, `ContentServerDirectory.GetServersForSteamPipe` (cell_id), `ContentServerDirectory.GetManifestRequestCode` (app_id, depot_id, manifest_id, app_branch="public").
- Mensaje clásico con job: `ClientGetDepotDecryptionKey` (5438) `{depot_id, app_id}` + `jobid_source` → 5439 con `jobid_target`, `depot_encryption_key` (32 B). Anónimo obtiene la de 108600.

## 8. PICS, ramas y acceso anónimo  **[verificado]** (2026-11-10)
- `ClientPICSAccessTokenRequest` (8905) → token por app (PZ: 0, no hace falta) → `ClientPICSProductInfoRequest` (8903, `single_response`) → 8904.
  Si `buffer` vacío: `http://<http_host>/appinfo/<appid>/sha/<sha hex>.txt.gz` (formato sacado de steamclient.so). Contenido = KeyValues texto.
- `appinfo.depots.branches.<rama>` = buildid, descripción, `pwdrequired`. `appinfo.depots.<depot>.manifests.<rama>` = `{gid, size, download}`.
- **Versión del juego = rama**. Ramas actuales (iguales en 380870 y 108600): `public`, `pre-release`, `42.21`, `42.19`, `legacy41` (Build 41.78.21). Ninguna con contraseña.
- `GetManifestRequestCode` con `app_branch=<rama>` funciona para ramas no públicas (probado `legacy41` en anónimo).
- Depots:
  - 380870 servidor: 380871 (común, sin oslist) + 380872 mac / 380873 linux / 380874 windows. 1004-1006 = Steamworks redist. **Anónimo: clave OK en todos**.
  - 108600 juego: 108601 (sin manifests), 108602 mac / 108603 linux / 108604 windows; 228983-7 = redist Windows. **Anónimo: eresult 15 (AccessDenied)** → requiere cuenta que lo tenga.
- Para instalar una versión: elegir rama → para cada depot cuyo `oslist` incluya el SO (o vacío) y tenga manifest en esa rama → descargar ese gid.
- Pendiente: si se pueden pedir manifests **antiguos fuera de las ramas** (fijar build arbitrario) — no probado.

## 9. Versiones de mods  **[verificado]** (anónimo)
- `PublishedFile.GetChangeHistory {publishedfileid, count}` → `changes[]` con `timestamp`, `change_description` (changelog), `manifest_id` por versión, y `snapshot_game_branch_min/max` (rango de ramas del juego; vacío en los mods probados).
- `GetManifestRequestCode` acepta manifests **antiguos** del item (probado 8 versiones atrás de 3833349401) y sus chunks siguen en el CDN: versión vieja reconstruida completa (182 archivos, SHA1 OK).
- ⇒ Un modpack puede fijar `(publishedfileid, manifest_id)` y ser reproducible, como los hashes de `.mrpack`.

## 10. Permisos y launch  **[verificado/observado]**
- Los manifests de PZ y Steamworks traen flags 0 (archivo) / 64 (dir); sin bit ejecutable ni symlinks.
- SteamCMD en Linux crea **todos** los archivos con 0777 & ~umask (→ `rwxr-xr-x`), independientemente de los flags. Reimplementación: crear archivos 0755 (o al menos los ejecutables de launch).
- `installscript` con claves `chmod` existe en steamclient.so pero PZ no lo usa (appinfo sin installscript).
- Launch (appinfo `config.launch`): Linux `projectzomboid.sh` (y `-safemode`), Windows `ProjectZomboid64.exe`/`.bat`, macOS `Project Zomboid.app`. `installdir` = `ProjectZomboid`.

## 11. Cambio de rama  **[verificado]**
- SteamCMD (`app_update <id> -beta <rama> [-betapassword X]`, código en steamconsole.so ~0x9ed7a-0x9f4b2):
  1. parsea `-beta`/`-betapassword`; 2. refresca appinfo (PICS); 3. si hay contraseña → `CheckAppBetaPassword` (EMsg ClientCheckAppBetaPassword) y espera; 4. fija la rama localmente (vacía o `default` → la limpia); 5. lanza la actualización.
- **No hay estado de rama en el servidor**: es elección local. Log: `AppID 380870 beta changed: legacy41`.
- Persistencia en `steamapps/appmanifest_<id>.acf`: `UserConfig.BetaKey = "legacy41"` (rama deseada) y `TargetBuildID = 24928750` (= buildid de `legacy41` en PICS). Rama pública → `UserConfig` vacío.
  Se espera que al terminar `MountedConfig.BetaKey` + `buildid` reflejen lo instalado [no verificado: se cortó la descarga].
- Reimplementación: rama = qué `manifests.<rama>.gid` usar por depot + `app_branch` en `GetManifestRequestCode`. Ramas con `pwdrequired` necesitarían CheckAppBetaPassword (PZ no tiene).
- Importar una instalación de Steam: leer `UserConfig.BetaKey`/`MountedConfig.BetaKey` y `buildid` del acf; comparar con `depots.branches.<rama>.buildid` de PICS para saber versión y si hay actualización.

## Pendiente
- Actualizar una instancia entre builds: diff de manifests y reutilizar chunks existentes (SteamCMD reporta "reuse/delta").
- Dependencias de mods (`children` en GetDetails con `includechildren`).
- Heartbeat (`ClientHeartBeat` 703 cada `heartbeat_seconds`) para sesiones largas: aún no implementado (las pruebas duran < 9 s).
- Login de usuario + Steam Guard + refresh token (para 108600 cliente).
- Valores reales de `client_os_type` y `protocol_version` del binario (hoy son hipótesis aceptadas por el servidor).
