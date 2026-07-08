import re

with open('src/composables/useP2PSync.ts', 'r') as f:
    content = f.read()

# 1. Add keepaliveTimer
content = content.replace('let reconnectTimer: number | null = null;', 
    'let reconnectTimer: number | null = null;\n  let keepaliveTimer: number | null = null;')

# 2. Add setupKeepalive
keepalive_fn = """
  // --- Keepalive Ping ---
  function setupKeepalive() {
    if (keepaliveTimer !== null) {
      window.clearInterval(keepaliveTimer);
      keepaliveTimer = null;
    }
    if (p2pConnected.value) {
      keepaliveTimer = window.setInterval(async () => {
        try {
          await invoke('p2p_sync_ping');
        } catch (e) {
          logger.warn('P2P keepalive ping failed, connection lost:', e);
          p2pConnected.value = false;
          if (keepaliveTimer !== null) {
            window.clearInterval(keepaliveTimer);
            keepaliveTimer = null;
          }
          if (appStore.p2pAutoSyncEnabled) {
            autoReconnect(0);
          }
        }
      }, 30000); // 30 seconds
    }
  }

  // --- Auto Sync ---"""
content = content.replace('  // --- Auto Sync ---', keepalive_fn)

# 3. Call setupKeepalive in setupAutoSync
content = content.replace('setupAutoSync();', 'setupAutoSync();\n      setupKeepalive();')

# 4. Clear keepaliveTimer in disconnectP2P
disconnect_clear = """      if (autoSyncTimer !== null) {
        window.clearInterval(autoSyncTimer);
        autoSyncTimer = null;
      }
      if (keepaliveTimer !== null) {
        window.clearInterval(keepaliveTimer);
        keepaliveTimer = null;
      }"""
content = content.replace('      if (autoSyncTimer !== null) {\n        window.clearInterval(autoSyncTimer);\n        autoSyncTimer = null;\n      }', disconnect_clear)

with open('src/composables/useP2PSync.ts', 'w') as f:
    f.write(content)
