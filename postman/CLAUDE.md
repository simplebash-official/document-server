# CLAUDE.md (postman/)

## Postman collection

`document-server.postman_collection.json` (one folder per module: `Health`, `Render`, `Templates`, `Documents`, `Barcodes`, `QRCodes`) plus `development.postman_environment.json`/`production.postman_environment.json` (each holding `baseUrl`, `templateKey`, `stickerKey`, and `documentKey`). Hand-maintained — update it in the same change as any route addition, same convention as `../backend/postman/`. Unlike that collection, there's no `accessToken`/Bearer auth variable here — nothing in this service needs one.
