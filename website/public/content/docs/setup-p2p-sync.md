# Setup P2P Sync

One of Synabit's most powerful features is its **Serverless P2P (Peer-to-Peer) Sync**. This technology allows your devices to communicate and synchronize your digital brain directly with each other over your Local Area Network (LAN), without ever routing your data through a centralized cloud server.

This guide will walk you through setting up sync between two devices (e.g., your laptop and your mobile phone).

---

## How it Works

When you enable P2P Sync, your devices discover each other securely using local network protocols. Once paired, changes made on one device are instantly pushed to the other device via an encrypted tunnel.

- **Zero Cloud:** Your data never leaves your Wi-Fi network.
- **End-to-End Encrypted:** All data transferred between peers is encrypted using industry-standard cryptography.
- **Conflict Resolution:** Synabit uses advanced CRDTs (Conflict-free Replicated Data Types) to merge edits flawlessly, even if you were offline.

---

## Step-by-Step Setup

To sync two devices, you need to designate one as the **Host** (usually your primary computer) and the other as the **Client** (e.g., your phone or a secondary laptop).

### Step 1: Start Broadcasting on Device A (Host)
1. Open Synabit on your primary device.
2. Navigate to **Settings** (the gear icon in the bottom left corner).
3. Click on the **Sync** tab.
4. Toggle the switch to enable **Local Network Sync**.
5. Synabit will generate a secure **Pairing Code** and display a QR code on the screen. Keep this screen open.

### Step 2: Connect from Device B (Client)
1. Open Synabit on your second device.
2. Go to **Settings** > **Sync**.
3. Under the *Connect to a Device* section, choose one of the following methods:
   - **Scan QR Code:** If Device B has a camera, simply scan the QR code displayed on Device A.
   - **Manual Entry:** Click "Enter Pairing Code" and type the code displayed on Device A.
4. Click **Connect**.

### Step 3: Approve the Connection
1. Look back at Device A. You will see a prompt asking: *"Device B is trying to connect. Do you want to allow this?"*
2. Verify the device name and click **Approve**.
3. Your devices are now permanently paired! They will automatically sync in the background whenever they are on the same Wi-Fi network.

---

## Using the Managed Sync Server (Pro Tier)

If you need your devices to sync when they are *not* on the same Wi-Fi network (e.g., syncing your home PC with your work laptop across the internet), you can use the **Managed Sync Server**.

This feature is available exclusively for **Synabit Pro** users.

1. Go to **Settings** > **Sync**.
2. Select **Cloud Sync (Pro)**.
3. Log in with your Synabit Pro account credentials.
4. Your data remains End-to-End Encrypted before it leaves your device. The server only routes encrypted packets and cannot read your notes.

---

## Troubleshooting

If your devices are failing to connect over the local network, try these common fixes:

- **Same Network:** Ensure both devices are connected to the exact same Wi-Fi network or router. Guest networks often isolate devices from each other.
- **Firewall Warnings:** On Windows, Windows Defender might block Synabit from broadcasting. Make sure to click "Allow Access" if a firewall prompt appears. If you missed it, go to Windows Security > Firewall and allow Synabit.
- **VPNs:** If one device is connected to a VPN, it might be on a different virtual subnet. Try pausing your VPN temporarily during the initial pairing process.

---

## What's Next?

With your digital brain now safely syncing across your devices, you can explore the core features of Synabit:

- [Note Vault](/docs/note-vault)
- [Task Management](/docs/task-management)
