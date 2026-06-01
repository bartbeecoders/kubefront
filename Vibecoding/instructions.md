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