Can you add a connection to the kubefront to be able to connect to an Azure AKS cluster? With and az login account or conneciton.


$nasm = (Get-ChildItem "C:\Program Files\NASM","C:\Program Files (x86)\NASM" -Filter nasm.exe -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1).DirectoryName
[Environment]::SetEnvironmentVariable("Path", [Environment]::GetEnvironmentVariable("Path","User") + ";$nasm", "User")
"NASM added: $nasm"