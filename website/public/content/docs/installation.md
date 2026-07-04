# Installation Guide

Setting up Synabit is designed to be as simple as possible. Since Synabit is built with Tauri and Rust, it is highly optimized, lightweight, and runs natively on your operating system.

Follow the instructions below to install Synabit on your preferred platform.

---

## Windows

We provide pre-built `.exe` and `.msi` installers for Windows 10 and 11.

1. Go to the [Releases](https://github.com/kid0604/synabit/releases) page on our GitHub repository.
2. Download the latest `Synabit_Setup_x64.exe` file.
3. Double-click the downloaded file and follow the installation wizard.
4. Once installed, launch Synabit from your Start Menu.

## macOS

Synabit supports both Apple Silicon (M1/M2/M3) and Intel-based Macs.

1. Navigate to the [Releases](https://github.com/kid0604/synabit/releases) page.
2. Download the appropriate `.dmg` file:
   - For Apple Silicon: Download `Synabit_aarch64.dmg`
   - For Intel Macs: Download `Synabit_x64.dmg`
3. Open the `.dmg` file and drag the Synabit icon into your `Applications` folder.
4. Launch Synabit from Launchpad or Spotlight.
> **Note:** If macOS prevents you from opening the app because it's from an "unidentified developer", simply right-click the app in Finder and select **Open**.

## Linux

We offer an AppImage for universal Linux compatibility, as well as a `.deb` package for Debian/Ubuntu-based distributions.

### Using AppImage (Recommended for all distros)
1. Download the `Synabit.AppImage` file from the [Releases](https://github.com/kid0604/synabit/releases) page.
2. Make the file executable:
   ```bash
   chmod +x Synabit.AppImage
   ```
3. Double-click the file or run it from the terminal to launch the app.

### Using .deb (Debian / Ubuntu)
1. Download the `synabit_amd64.deb` file.
2. Install it using `dpkg`:
   ```bash
   sudo dpkg -i synabit_amd64.deb
   sudo apt-get install -f # to fix any missing dependencies
   ```
3. Launch Synabit from your application menu.

---

## Build from Source (Free Tier)

If you prefer the **Free (Open Source)** tier and want to compile the app yourself, make sure you have the following prerequisites installed:
- [Node.js](https://nodejs.org) (v18 or newer)
- [Rust](https://rustup.rs/) (latest stable)
- Build tools (e.g., build-essential on Linux, Visual Studio C++ Build Tools on Windows, Xcode on macOS)

### Steps to build
1. Clone the repository:
   ```bash
   git clone https://github.com/kid0604/synabit.git
   cd synabit
   ```
2. Install frontend dependencies:
   ```bash
   npm install
   ```
3. Build the Tauri application:
   ```bash
   npm run tauri build
   ```
The compiled binaries will be available in the `src-tauri/target/release/bundle/` directory.

---

## What's Next?

Now that Synabit is installed, it's time to set up syncing so your digital brain is available on all your devices.

- [Setup P2P Sync](/docs/setup-p2p-sync)
- [Your First Note Vault](#)
