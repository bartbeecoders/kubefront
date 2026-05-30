# KubeFront Build Scripts

This folder contains helper scripts to build release versions of KubeFront and sign them with a **self-signed certificate**.

> **Important**: Self-signed certificates are great for testing and internal use.  
> For public distribution, you should use a certificate from a trusted Certificate Authority (DigiCert, Sectigo, GlobalSign, etc.).

---

## Windows (Recommended)

Use `build-release.ps1` when building natively on Windows.

### Prerequisites

- Rust (via rustup)
- Windows SDK (provides `signtool.exe`)
- PowerShell

### Usage

```powershell
# Normal release build + signing
.\scripts\build-release.ps1

# Clean build
.\scripts\build-release.ps1 -Clean

# Custom certificate name
.\scripts\build-release.ps1 -CertName "My Company"
```

### What it does

1. Creates a self-signed code signing certificate (if one doesn't exist)
2. Runs `cargo build --release`
3. Signs the executable using `signtool`
4. Copies the signed binary to `dist/KubeFront.exe`

The signed file will be located at:

```
dist/KubeFront.exe
```

---

## Linux / macOS (Cross Compilation)

Use `build-release.sh` when you want to cross-compile from Linux or macOS.

### Prerequisites

```bash
# Ubuntu / Debian
sudo apt install osslsigncode

# macOS
brew install osslsigncode
```

Also make sure you have the Windows target installed:

```bash
rustup target add x86_64-pc-windows-msvc
```

### Usage

```bash
# Normal build
./scripts/build-release.sh

# Clean build
./scripts/build-release.sh --clean
```

### Output

Signed executable will be placed in:

```
dist/KubeFront.exe
```

---

## Notes

- Both scripts generate a **self-signed** certificate valid for 5 years.
- On Windows, the certificate is stored in the Current User's Personal store.
- On Linux, the certificate + private key are stored in `scripts/certs/`.
- Users running a self-signed binary will see a Windows SmartScreen / "unknown publisher" warning.
- You can export the certificate from Windows Certificate Manager (`certmgr.msc`) if you want to share it.

---

## Future Improvement Ideas

- Support for real EV / OV code signing certificates
- Timestamp server configuration
- Automatic version embedding
- Creating `.msi` or `.zip` packages

Let us know if you need any of the above!