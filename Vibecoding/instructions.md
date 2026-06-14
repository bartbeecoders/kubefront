Create a K3S cluster frontend

- use rust
- should work on linux and windows
- support multiple clusters
- use a kubectl config file to connect to the clusters
- modern ui (web based ?)


Add a seperate settings page where you can configure the following:
- kubeconfig file path
- default namespace
- color theme (light/dark/custom)
- font size
- log level (debug/info/warn/error)


Allow for the management of multiple kubeconfig files/clusters and switch between them.


Can you add the other pages that are needed for the frontend?
- Clusters page
- Nodes page
- Pods page
- Services page
- Deployments page
- StatefulSets page
- DaemonSets page
- Jobs page
- CronJobs page
- ConfigMaps page
- Secrets page
- Namespaces page
- Storage page
- Network page
- Security page
- Monitoring page
- Logging page
- Settings page


Refactor this app to use a more modern ui (web based ?). Replace egui with a web-based ui.

Add a detail info pane for each resource type that shows the details of the selected resource.


Can you add following features:

- add a main dashboard page that shows all the configured clusters (with status, version, nodes, pods, etc.)
    - user can clis on a cluster card, which will bring him to the details page
- add ability to delete pods, restart pods
- add ability to delete deployments, restart deployments
- add ability to delete services
- add ability to delete configmaps
- add ability to delete secrets
- add ability to delete storage
- add ability to delete network
- add ability to delete security

In key places add a copy text feature to copy the text to the clipboard.


Add the ability to see CRDS


### World view

Add to each connection a location (city, country). 
On the dashboard show a world map and show the connections on this map based on the location.

Add also some other parameters to the connection:
- cluster type (K3S, K8S, AKS)
- Manufacturing plant
- Environment (dev, val, prod)