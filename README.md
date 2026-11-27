# Discord Rust

Cliente de escritorio y servidor local en Rust para los mensajes de texto de la API v10. El contrato se basa en los archivos developers/resources/message.mdx, developers/resources/channel.mdx y developers/reference.mdx de discord-api-docs-main.zip.

## Ejecutar

Desde esta carpeta, en dos terminales de PowerShell:

~~~powershell
cargo run --locked -p discord-text --bin discord-server
~~~

~~~powershell
cargo run --locked -p discord-text --bin discord-client
~~~

El servidor escucha en 127.0.0.1:8790. Se puede elegir otro puerto mediante DISCORD_LOCAL_PORT en ambas terminales. DISCORD_LOCAL_DB selecciona la ruta SQLite del servidor. Cada ventana crea una identidad local al entrar y guarda el token en memoria durante esa sesión. Volver a escribir el mismo nombre crea otra identidad. El historial persiste en disco.

## Cobertura

| Ruta | Operaciones |
| --- | --- |
| /local/sessions | POST crea una identidad local y entrega su token |
| /api/v10/guilds/{guild_id}/channels | GET enumera los canales y POST crea un canal de texto |
| /api/v10/channels/{channel_id}/messages | GET pagina el historial y POST publica texto |
| /api/v10/channels/{channel_id}/messages/{id} | GET consulta un mensaje, PATCH edita el texto y DELETE elimina el mensaje |

Las rutas v10 requieren un token local en la cabecera Authorization con el esquema Bearer. La creación y edición admiten el campo content. La paginación devuelve mensajes del más reciente al más antiguo, con un límite de 1 a 100 y los cursores before, after o around. La edición y la eliminación están restringidas al autor. El cliente muestra 50 mensajes por página y actualiza la página visible cada dos segundos.

La implementación tiene un único servidor lógico, sesiones locales y dos canales iniciales. Cualquier sesión local puede crear canales de texto. OAuth2, roles de servidor, Gateway, archivos adjuntos, llamadas y conexiones con cuentas oficiales quedan fuera de esta versión.

## Verificar

~~~powershell
cargo build --workspace --locked
python chat/verify_api.py
python verify.py
~~~

La primera prueba levanta el servidor en un puerto asignado por el sistema con una base temporal. La segunda compara el cálculo de progreso del actualizador con el de la instalación local de Discord.

## Componentes de Windows

El cliente arranca con las fuentes de egui y permite importar en local el icono de app.ico y los dos pesos de GG Sans de una instalación de Discord. Estos recursos son opcionales. Para importarlos, ejecuta `python chat/import_desktop_assets.py` con fontTools, Brotli y Pillow disponibles. El cliente los lee al arrancar, desde chat/assets o desde el directorio indicado por DISCORD_LOCAL_ASSETS. El tiempo de reintento tras un error sigue el cálculo de discord_overlay2/Backoff.js.

El ejecutable del actualizador admite `--display`. Lee un estado JSON por línea y devuelve el texto y los porcentajes de las barras que usa la pantalla de arranque. Por ejemplo, en PowerShell:

~~~powershell
'{"status":"update-failure","seconds":3}' | cargo run --locked -p discord-updater-progress -- --display
~~~

El cliente lee el color de fondo, el tamaño de ventana y el estado maximizado de %APPDATA%/discord/settings.json al arrancar.

## Aplicación de ejemplo e integraciones locales

Escribe / para abrir el selector de comandos y elige /test. La respuesta aparece en el historial de la ventana que lo solicitó, con la insignia APP y la opción Descartar. El diseño toma como referencia assets/getting-started-demo.gif del Example App. El texto utiliza un catálogo de mensajes en español o inglés, organizado según las convenciones de discord-intl.

| Archivo adjunto | Uso en Rust |
| --- | --- |
| discord-example-app-main.zip | Comando /test y respuesta con un componente Text Display |
| discord-interactions-js-main.zip | Verificación Ed25519 de los bytes originales del cuerpo precedidos por la cabecera de fecha |
| discord-api-spec-main.zip | Contrato de las respuestas PONG y de mensaje con componentes y comprobación de los campos de /test contra OpenAPI |
| discord-intl-main.zip | Catálogos .messages.json y sustitución del parámetro emoji |
| embedded-app-sdk-main.zip | RPC local GET_CHANNEL con correlación por nonce, metadatos del canal e historial |
| discord-grpc-master.zip | Servicio grpc.health.v1.Health con Check y Watch en el mismo puerto local |

POST /interactions valida la firma con DISCORD_INTERACTIONS_PUBLIC_KEY, una clave pública Ed25519 en hexadecimal. Si falta esa variable el webhook responde 401. El servidor se ejecuta en loopback y el comando del escritorio usa la ruta local con token de sesión. GET_CHANNEL se prueba por HTTP con POST /local/activities/rpc y el mismo token. No se necesita una aplicación registrada ni acceso a servidores externos para reproducir estas pruebas.

~~~powershell
cargo test --workspace --locked
python chat/verify_api.py "$env:USERPROFILE/Downloads/discord-api-spec-main.zip"
~~~
