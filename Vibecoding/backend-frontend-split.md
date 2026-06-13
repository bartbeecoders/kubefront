Can you split the kubefront app into 2 parts:

Backend:
- Rust app handling the communication to the kubernetes cluster
- multiple connections could be possible (multiple kubectml.config files)
- Provides a REST API for the frontend to consume
- this api will be then served by a reverse proxy on port 443 (https://server/k3s-server1/connection1/api)

Frontend:
- Rust app (same as today)
- can connect to the backend API (over port 443)
- can connect directly to the kubernetes cluster (over port 6443)

Overall scenario is the following:
- we have multiple kubernetes clusters istalled in an OT environment (globally dispersed)
- we can only communicate to these ot environments through a reverse proxy (port 443)
- the kubefront app acts as a multi cluster config tool