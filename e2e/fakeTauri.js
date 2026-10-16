/**
 * A Tauri backend in the page, for driving the front end in a browser.
 *
 * Put in before the app loads (`page.addInitScript`). It answers `invoke` the
 * way `@tauri-apps/api/mocks` does, from an in-memory vault, and keeps what the
 * app says: every command (`__fake.calls`), every one it has no answer for
 * (`__fake.unknown`), every log line (`__fake.log`), and the `[invariant]`
 * lines among them (`__fake.violations`) — the app's own checks, reported to
 * whoever drives it.
 *
 * Plain JavaScript, self-contained: it runs in the page, not in the test.
 */
(() => {
  const VAULT = '/vault';
  const now = '2026-10-16 08:00:00';

  /** The vault: node id → node, as the backend returns one. */
  const nodes = new Map();
  const note = (id, title, content, extra = {}) =>
    nodes.set(id, {
      id,
      node_type: extra.node_type || 'note',
      title,
      content,
      properties: extra.properties || { tags: [], pinned: false, full_width: false },
      created_at: now,
      updated_at: now,
      timestamp: 0,
      blocks: null,
    });
  note('Notes/welcome.md', 'Chào', '# Chào\n\nMột ghi chú có **đậm**, *nghiêng*, [link](https://example.com) và `code`.\n\n- một\n- hai\n\n- [ ] việc chưa làm\n- [x] việc đã làm\n');
  note('Notes/blocks.md', 'Các khối', '<details class="synabit-details">\n<summary>Toggle</summary>\n<div class="details-content">\n\nBên trong\n\n</div>\n</details>\n\nThread: <https://chat.google.com/room/x?cls=10>\n\n| A | B |\n| --- | --- |\n| 1 | `x` |\n\n```js\nconst a = 1;\n```\n\n> trích dẫn\n> - mục\n\n$$x^2$$ và chữ\n');
  note('Notes/2026-10-16.md', '2026-10-16', 'Hôm nay.\n', { properties: { tags: ['daily'], date: '2026-10-16' } });
  note('Tasks/goi-cmc.md', 'Gọi CMC', 'Gọi điện và email CMC phối hợp golive.\n', { node_type: 'task', properties: { status: 'todo', priority: 'high' } });

  const summary = (n) => ({
    id: n.id,
    node_type: n.node_type,
    title: n.title,
    preview: n.content.slice(0, 80),
    properties: n.properties,
    created_at: n.created_at,
    updated_at: n.updated_at,
  });

  /** Syn's conversations: id → { meta, messages }. */
  const conversations = new Map();

  /** `@tauri-apps/plugin-store`: one store per path, by resource id. */
  const stores = new Map();
  const storeFor = (rid) => stores.get(rid) || new Map();
  let nextRid = 1;
  const settings = new Map([
    ['vaultPath', VAULT],
    ['onboardingDone', true],
    ['language', 'vi'],
  ]);

  const fake = {
    calls: [],
    unknown: {},
    log: [],
    violations: [],
    listeners: new Map(),
  };
  window.__fake = fake;

  /** What each command answers. A command not here answers `null`, and is counted. */
  const handlers = {
    // ── Plugins ──
    'plugin:store|load': ({ path }) => {
      const rid = nextRid++;
      stores.set(rid, path.includes('settings') || path.includes('.dat') || path.includes('store') ? settings : new Map());
      return rid;
    },
    'plugin:store|get_store': () => null,
    'plugin:store|get': ({ rid, key }) => {
      const s = storeFor(rid);
      return [s.get(key) ?? null, s.has(key)];
    },
    'plugin:store|set': ({ rid, key, value }) => void storeFor(rid).set(key, value),
    'plugin:store|has': ({ rid, key }) => storeFor(rid).has(key),
    'plugin:store|delete': ({ rid, key }) => storeFor(rid).delete(key),
    'plugin:store|keys': ({ rid }) => [...storeFor(rid).keys()],
    'plugin:store|values': ({ rid }) => [...storeFor(rid).values()],
    'plugin:store|entries': ({ rid }) => [...storeFor(rid).entries()],
    'plugin:store|length': ({ rid }) => storeFor(rid).size,
    'plugin:store|save': () => null,
    'plugin:store|reload': () => null,
    'plugin:store|clear': ({ rid }) => storeFor(rid).clear(),
    'plugin:store|reset': () => null,
    'plugin:log|log': ({ message, level }) => {
      fake.log.push({ level, message });
      const at = String(message).indexOf('[invariant]');
      if (at >= 0) {
        try {
          fake.violations.push(JSON.parse(String(message).slice(at + 11).trim()));
        } catch {
          fake.violations.push({ invariant: '?', detail: { line: String(message).slice(0, 200) } });
        }
      }
      return null;
    },
    'plugin:event|listen': ({ event, handler }) => {
      if (!fake.listeners.has(event)) fake.listeners.set(event, []);
      fake.listeners.get(event).push(handler);
      return handler;
    },
    'plugin:event|unlisten': () => null,
    'plugin:event|emit': () => null,
    'plugin:event|emit_to': () => null,
    'plugin:os|platform': () => 'macos',
    'plugin:os|os_type': () => 'macos',
    'plugin:os|hostname': () => 'test-mac',
    'plugin:os|locale': () => 'vi-VN',
    'plugin:app|version': () => '0.9.27',
    'plugin:app|name': () => 'Synabit',
    'plugin:path|resolve_directory': () => '/home',
    'plugin:dialog|ask': () => true,
    'plugin:dialog|confirm': () => true,
    'plugin:dialog|message': () => null,
    'plugin:opener|open_url': () => null,
    'plugin:window|is_focused': () => true,
    'plugin:window|is_visible': () => true,
    'plugin:window|is_fullscreen': () => false,
    'plugin:window|is_maximized': () => false,
    'plugin:window|scale_factor': () => 2,
    'plugin:window|inner_size': () => ({ width: 1280, height: 800 }),
    'plugin:window|outer_size': () => ({ width: 1280, height: 800 }),
    'plugin:autostart|is_enabled': () => false,

    // ── The vault ──
    resolve_mobile_vault_path: () => VAULT,
    scan_all_nodes: () => ({ scanned: nodes.size, errors: [], skipped: [] }),
    scan_specific_nodes: () => ({ scanned: 0, errors: [], skipped: [] }),
    get_node_summaries: ({ nodeType } = {}) => [...nodes.values()].filter((n) => !nodeType || n.node_type === nodeType).map(summary),
    get_nodes: ({ nodeType } = {}) => [...nodes.values()].filter((n) => !nodeType || n.node_type === nodeType),
    get_node: ({ id, nodeId, path } = {}) => nodes.get(id || nodeId || path) ?? null,
    get_events_in_range: () => [],
    count_inbox_caps: () => 0,
    feed_get_total_unread: () => 0,
    get_chat_history: () => [],
    take_quick_question: () => null,
    get_app_lock_config: () => ({ is_enabled: false, app_lock_active: false, protected_apps: [], protected_notes: [], auto_lock_timeout_secs: 0 }),
    diagnostics_info: () => ({ available: false }),
    check_e2ee_status: () => ({ key_available: false, needs_setup: false }),
    get_nexus_graph_data: () => ({
      nodes: [...nodes.values()].map((n) => ({ id: n.id, item_type: n.node_type, title: n.title, tags: n.properties.tags || [] })),
      links: [{ source: 'Notes/welcome.md', target: 'Notes/blocks.md' }],
    }),
    timeline_reading_left: () => ({ pending: 0, total: 0, done: 0 }),
    import_handoff_captures: () => 0,
    timeline_extract_status: () => ({
      config: {}, syn_enabled: false, provider: 'none', local: false, model: null, desktop: true, running: false,
      unreadable: [], proposals: [], people: [], categories: [], moments: {},
    }),
    timeline_read_line: () => ({ read: null, model: null, refused: null }),
    syn_get_settings: () => ({ provider: 'open_ai_compat', default_model: null }),
    page_alive: () => null,
    rename_node_file: ({ oldRelPath, newName }) => {
      const dir = String(oldRelPath || 'Notes/x.md').split('/').slice(0, -1).join('/') || 'Notes';
      const next = `${dir}/${String(newName || 'Untitled').replace(/[\\/:*?"<>|]/g, '-').slice(0, 80)}.md`;
      const n = nodes.get(oldRelPath);
      if (n) {
        nodes.delete(oldRelPath);
        nodes.set(next, { ...n, id: next });
      }
      return next;
    },
    run_node_query: () => ({ columns: [], rows: [], total: 0 }),
    search_notes: () => ({ results: [...nodes.values()].slice(0, 5).map((n) => ({ id: n.id, title: n.title })) }),
    search_nexus: () => ({ results: [] }),
    search_nexus_ids: () => [],
    file_tag_counts: () => [],
    get_file_sources: () => [],
    list_board_libraries: () => [],
    syn_check_status: () => ({ connected: false, version: null, url: '', can_manage_models: false }),
    syn_list_threads: () => [],
    safe_status: () => ({ exists: false, unlocked: false, has_secret_key: false }),
    timeline_media_status: () => ({
      config: { transcripts: false, transcribe_url: '', transcribe_model: '', captions: false, caption_model: '' },
      desktop: true, provider: 'none', local_provider: false, pending_transcripts: 0, pending_captions: 0, done: 0, too_large: 0, running: false,
    }),
    migrate_daily_note_dates: () => ({ changed: 0 }),
    migrate_quickcap_storage: () => ({ changed: 0 }),
    migrate_finance_storage: () => ({ changed: 0 }),
    migrate_people_storage: () => ({ interactions_moved: 0 }),
    get_linked_nodes: () => [],
    get_tasks_in_range: () => [...nodes.values()].filter((n) => n.node_type === 'task').map(summary),
    get_migration_flag: () => 'done',
    query_file_page: () => ({ files: [], total: 0 }),
    query_file_ids: () => [],
    list_thumbnails: () => [],
    syn_footing_tally: () => ({ measured: 0, grounded: 0, inferred: 0, guessing: 0, unmeasured: 0 }),
    syn_thread_usage: () => ({ runs_total: 0, runs_in_a_thread: 0, runs_that_wrote_back: 0 }),
    syn_staged_list: () => [],
    last_contact_dates: () => ({}),
    list_file_collections: () => [],
    file_text_backlog: () => 0,
    list_queued_captures: () => [],
    scan_whiteboards: () => [],
    search_event_occurrences: () => [],
    list_calendar_subscriptions: () => [],
    list_cameras: () => [],
    archive_done_nodes: () => 0,
    search_quickcaps: () => ({ results: [], total_count: 0, query_time_ms: 0 }),
    feed_refresh: () => ({ totalFetched: 0, totalNew: 0, errors: [] }),
    syn_get_instructions: () => '',
    syn_instructions_path: () => '/vault/SYN.md',
    syn_instructions_template: () => '# Làm việc với tao\n\n',
    syn_save_instructions: () => null,
    create_whiteboard: ({ title } = {}) => {
      const id = `board-${Math.random().toString(36).slice(2, 8)}`;
      return { id, title: title || 'Untitled', tags: [], content: '{"nodes":[],"edges":[]}', path: `Whiteboards/${id}.json`, created_at: now, updated_at: now };
    },
    syn_list_conversations: () => [...conversations.values()].map((c) => c.meta),
    syn_create_conversation: ({ title } = {}) => {
      const id = `conv-${Math.random().toString(36).slice(2, 8)}`;
      const meta = { id, title: title || 'New Conversation', model: null, provider: null, message_count: 0, created_at: now, updated_at: now, pinned: false };
      conversations.set(id, { meta, messages: [] });
      return meta;
    },
    syn_get_conversation: ({ conversationId, id } = {}) => conversations.get(conversationId || id) ?? null,
    syn_delete_conversation: ({ conversationId, id } = {}) => void conversations.delete(conversationId || id),
    syn_rename_conversation: ({ conversationId, id, title } = {}) => {
      const c = conversations.get(conversationId || id);
      if (c) c.meta.title = title;
    },
    syn_list_runs: () => [],
    syn_waiting: () => [],
  };

  const handle = (cmd, args) => {
    const h = handlers[cmd];
    if (h) return h(args || {});
    // Saving: any command that writes a node, kept so the app reads back what it wrote.
    if (/^(write|save|update|create)_node/.test(cmd) && args) {
      const id = args.id || args.nodeId || args.path || args.metadata?.id;
      const content = args.content ?? args.body ?? args.metadata?.content;
      if (id && typeof content === 'string') {
        const n = nodes.get(id) || { id, node_type: 'note', title: id, properties: {}, created_at: now, timestamp: 0, blocks: null };
        nodes.set(id, { ...n, ...(args.metadata || {}), content, updated_at: now });
      }
      return nodes.get(id) ?? null;
    }
    fake.unknown[cmd] = (fake.unknown[cmd] || 0) + 1;
    // The kind the front end's own types say it answers (`e2e/app.ts`).
    const kind = (window.__shapes || {})[cmd];
    if (kind === 'array') return [];
    if (kind === 'string') return '';
    if (kind === 'number') return 0;
    if (kind === 'boolean') return false;
    if (kind === 'object') return {};
    // What a command answers is usually in its name.
    if (/^(list|search|query)_|_list$|(^|_)get_\w+s$|_(list|search)_/.test(cmd)) return [];
    if (/^count_|_count$/.test(cmd)) return 0;
    if (/(config|settings|status|info|stats)$/.test(cmd)) return {};
    if (/^(is|has|can)_/.test(cmd)) return false;
    return null;
  };

  const callbacks = new Map();
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    transformCallback(callback, once = false) {
      const id = window.crypto.getRandomValues(new Uint32Array(1))[0];
      callbacks.set(id, (data) => {
        if (once) callbacks.delete(id);
        return callback && callback(data);
      });
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    runCallback(id, data) {
      const cb = callbacks.get(id);
      if (cb) cb(data);
    },
    callbacks,
    convertFileSrc(path) {
      return `asset://localhost/${encodeURIComponent(path)}`;
    },
    async invoke(cmd, args) {
      fake.calls.push(cmd);
      return handle(cmd, args);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_OS_PLUGIN_INTERNALS__ = { os_type: 'macos', platform: 'macos', family: 'unix', version: '15.0', arch: 'aarch64', eol: '\n', exe_extension: '' };

  /** Raise an event as the backend would (`emit` from Rust). */
  fake.emit = (event, payload) => {
    for (const id of fake.listeners.get(event) || []) window.__TAURI_INTERNALS__.runCallback(id, { event, payload, id: 0 });
  };
})();
