# CLAUDE.md (postman/)

## Postman collection

`pdf-server.postman_collection.json` (one folder per module, mirroring the spec's Phase 1/2/3 route-surface notes in the root CLAUDE.md) plus `development.postman_environment.json`/`production.postman_environment.json` (each holding `baseUrl` and a sample `templateKey`). Hand-maintained — update it in the same change as any route addition, same convention as `../backend/postman/`. Unlike that collection, there's no `accessToken`/Bearer auth variable here — nothing in this service needs one.
