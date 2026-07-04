-- Bảng licenses: quản lý cả trial lẫn paid (trial cũng là 1 loại license)
CREATE TABLE licenses (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    license_key   TEXT UNIQUE,                     -- SYNC-XXXX hoặc NULL (trial tự sinh)
    type          TEXT NOT NULL,                   -- 'trial' | 'pro_monthly' | 'pro_annual'
    status        TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'expired' | 'revoked'
    max_devices   INTEGER NOT NULL DEFAULT 10,     -- Trial = 1, Pro = 10
    expires_at    DATETIME NOT NULL,
    customer_email TEXT,
    payment_id    TEXT,                            -- Polar/Stripe subscription ID
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Bảng devices: theo dõi thiết bị đã kích hoạt (dùng chung cho trial + pro)
CREATE TABLE devices (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    license_id     INTEGER NOT NULL REFERENCES licenses(id) ON DELETE CASCADE,
    hwid           TEXT NOT NULL,
    device_name    TEXT,
    activated_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_heartbeat DATETIME,
    UNIQUE(license_id, hwid)
);

CREATE INDEX idx_licenses_key ON licenses(license_key);
CREATE INDEX idx_devices_hwid ON devices(hwid);
