# Privacy Policy — Synabit

**Last updated:** July 25, 2026  
**Effective date:** July 25, 2026

This Privacy Policy describes how Synabit ("we", "us", "our") handles your information when you use the Synabit desktop and mobile application ("the App").

---

## 1. Our Commitment

Synabit is built on a **local-first** architecture. We prioritize your privacy and minimize the data we collect. However, to provide essential features like device synchronization, map routing, and license verification, the App must process certain metadata and communicate with specific external services.

This policy outlines exactly what data leaves your device, where it goes, and how it is protected.

## 2. Information We Process

### 2.1 Your Content (Local & E2EE)

Your actual content—including notes, tasks, files, and Whiteboard drawings—is **stored locally** on your device's filesystem. 

When you use our **Synabit Sync Relay** (`sync.synabit.net`) to synchronize devices, your content is **End-to-End Encrypted (E2EE)** using AES-256-GCM before it ever leaves your device. 
- We do **not** have the decryption keys.
- We do **not** have access to your readable content at any time.

### 2.2 Metadata & Identifiers Sent to Our Servers

To operate the service and prevent abuse, the App transmits the following metadata to our servers:

| Data Type | Sent To | Purpose | Retention |
|---|---|---|---|
| **Sync Payload (E2EE)** | `sync.synabit.net` | Temporarily routing encrypted sync data between your devices. | Deleted immediately after delivery, or discarded after a short expiry if undeliverable. |
| **Sync Metadata** | `sync.synabit.net` | Routing logic (IP addresses, payload size, connection timestamps). | Ephemeral; stored only in transient memory during the connection. |
| **Hardware ID (HWID)** | `license.synabit.net` | A cryptographically hashed identifier of your device to enforce device limits on your license. | Retained as long as your license/device is active. |
| **Device Name** | `license.synabit.net` | Your OS device name (e.g., "John's iPhone") to help you manage your active devices in the license portal. | Retained as long as your license/device is active. |
| **IP Address** | `license.synabit.net` | Fraud prevention and rate limiting. | Retained in transient access logs for up to 30 days. |

### 2.3 Third-Party Services

Depending on the features you use and the platform you are on, the App communicates with the following third-party processors:

- **Mapping Providers (Optional)**: If you use map, geolocation, or routing features, your IP address and requested geographic coordinates are sent to OpenStreetMap (OSM), OSRM, or Nominatim to render map tiles and calculate routes.
- **GitHub (Desktop Only)**: The desktop application checks GitHub for application updates, which exposes your IP address to GitHub's servers during the check.
- **Payment Processors**: If you purchase a subscription, payments are processed by third parties (e.g., Stripe, Apple, Google). We do not store your payment details.

## 3. How We Use Information

We use the metadata and identifiers described above strictly for the following purposes:
- Delivering your End-to-End Encrypted sync payloads to your other devices.
- Verifying your license status and enforcing device limits.
- Providing customer support (using information you voluntarily share).

We do **not** sell, rent, or share your data with advertising networks or data brokers. We do **not** track your usage behavior or employ analytics/telemetry within the App.

## 4. Data Security

- **Encryption**: All sync payloads transmitted through our relay are End-to-End Encrypted.
- **Local Storage**: OAuth tokens and encryption keys are stored in your operating system's native secure storage (macOS Keychain, Windows Credential Manager, Android Keystore).
- **Authentication**: The App uses PKCE for OAuth flows to prevent authorization code interception.

## 5. Data Retention & Deletion

Because Synabit does not require a traditional user account to function, the deletion process depends on the type of data:

- **Your Local Content**: Stored entirely on your device. Delete your vault folder to remove all content.
- **App Data & Settings**: Uninstall the App to remove all local cached data, search indices, and settings.
- **License & Device Records**: We retain your hashed Hardware ID, Device Name, and associated license status on our license server to manage your subscription. To request the permanent deletion of your license records, please contact us at **privacy@synabit.net** with your License Key.

## 6. Children's Privacy

Synabit is not directed at children under 13. We do not knowingly collect information from children under 13.

## 7. International Users

If you use Mapping features, your data is subject to those providers' own privacy practices and data processing locations. Metadata processed by our servers (`sync.synabit.net` and `license.synabit.net`) may be transferred to and processed in regions outside your country of residence.

## 8. Changes to This Policy

We may update this Privacy Policy from time to time to reflect changes in our data processing practices. We will notify users of material changes through the App or on our website. 

## 9. Contact Us

If you have questions or deletion requests regarding this Privacy Policy:

- **Email**: privacy@synabit.net
- **GitHub**: https://github.com/synabit/synabit/issues

---

*This Privacy Policy is provided in compliance with the Google Play Developer Policies and applicable data protection regulations.*
