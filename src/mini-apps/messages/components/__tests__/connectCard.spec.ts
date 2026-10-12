import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import type { DeclaredView } from '../../declared';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn().mockResolvedValue('/Users/me/Downloads') }));

import ConnectCard from '../ConnectCard.vue';

const calendar: DeclaredView = {
  node_id: 'SynConnectors/Calendar.md',
  title: 'Calendar (CalDAV)',
  manifest: {
    id: 'calendar',
    name: 'Calendar (CalDAV)',
    version: 1,
    provenance: 'bundled',
    base_url: '{{input.server}}',
    auth: { type: 'basic', user: '{{input.user}}', secret: 'app_password' } as never,
    inputs: [
      { name: 'server', kind: 'address', label: { en: 'Calendar address', vi: 'Địa chỉ lịch' }, example: 'https://cloud.example.com/dav/' },
      { name: 'user', kind: 'account', label: { en: 'User name', vi: 'Tên đăng nhập' } },
      {
        name: 'app_password',
        kind: 'secret',
        label: { en: 'App password', vi: 'Mật khẩu ứng dụng' },
        guide: { steps: { en: ['Settings → Security → Create new app password'], vi: ['Cài đặt → Bảo mật → Tạo mật khẩu ứng dụng'] } },
      },
    ],
    operations: [{ name: 'events', description: 'Events.', effect: 'read', call: { method: 'REPORT', path: '/' } }],
  },
  problems: [],
  standing: { state: 'new' },
  instance: null,
  effects: {},
  secret_here: false,
};

const show = () => mount(ConnectCard, { props: { vaultPath: '/v', connectorId: 'calendar' }, global: { plugins: [i18n] } });

describe('The connect card', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_manifest_list' ? [calendar] : null));
  });

  it('asks only what the person knows, in their language, with where to find it', async () => {
    const w = show();
    await flushPromises();
    const text = w.text();
    expect(w.find('[data-test="input-server"]').exists()).toBe(true);
    expect(w.find('[data-test="input-user"]').exists()).toBe(true);
    expect(w.find('[data-test="input-app_password"] input').attributes('type')).toBe('password');
    for (const word of ['YAML', 'host', 'secret', 'MCP', 'endpoint', 'network', 'certificate']) {
      expect(text.toLowerCase()).not.toContain(word.toLowerCase());
    }
    await w.find('[data-test="guide-app_password"]').trigger('click');
    expect(w.text()).toContain('Create new app password');
    expect(w.find('[data-test="connect-what"]').text()).toContain('sends nothing');
  });

  it('connects in one press, with the key going straight to Rust, and says it worked', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_manifest_list') return [calendar];
      if (cmd === 'syn_connect') return { ok: true, rows: 3, said: '' };
      return null;
    });
    const w = show();
    await flushPromises();
    await w.find('[data-test="input-server"] input').setValue('https://cloud.example.com/dav/');
    await w.find('[data-test="input-user"] input').setValue('me');
    await w.find('[data-test="input-app_password"] input').setValue('s3cret');
    expect(w.find('[data-test="connect-what"]').text()).toContain('cloud.example.com');
    await w.find('[data-test="connect"]').trigger('click');
    await flushPromises();
    const call = invoke.mock.calls.find(([c]) => c === 'syn_connect');
    expect(call?.[1]).toMatchObject({ manifestId: 'calendar', inputs: { server: 'https://cloud.example.com/dav/', user: 'me' }, secrets: { app_password: 's3cret' } });
    expect(w.find('[data-test="connect-ok"]').text()).toContain('3');
    expect(w.emitted('done')).toEqual([[true]]);
  });

  it('a rejected key is said plainly, the card stays, and the field is pointed at', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_manifest_list') return [calendar];
      if (cmd === 'syn_connect') return { ok: false, code: 'credential_rejected', said: '401' };
      return null;
    });
    const w = show();
    await flushPromises();
    await w.find('[data-test="connect"]').trigger('click');
    await flushPromises();
    expect(w.find('[data-test="connect-problem"]').text()).toContain("didn't accept");
    expect(w.emitted('done')).toBeUndefined();
    expect(w.find('[data-test="connect"]').exists()).toBe(true);
  });

  it('a self-signed server is trusted with one press, by its certificate', async () => {
    invoke.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === 'syn_manifest_list') return [calendar];
      if (cmd === 'syn_connect') return args.trustCert ? { ok: true, said: '' } : { ok: false, code: 'self_signed', cert: 'AbCd==', said: '' };
      return null;
    });
    const w = show();
    await flushPromises();
    await w.find('[data-test="connect"]').trigger('click');
    await flushPromises();
    await w.find('[data-test="trust"]').trigger('click');
    await flushPromises();
    expect(invoke.mock.calls.filter(([c]) => c === 'syn_connect').slice(-1)[0]?.[1]).toMatchObject({ trustCert: 'AbCd==' });
    expect(w.emitted('done')).toEqual([[true]]);
  });

  it('speaks Vietnamese when the app does', async () => {
    i18n.global.locale.value = 'vi';
    const w = show();
    await flushPromises();
    expect(w.text()).toContain('Địa chỉ lịch');
    expect(w.text()).toContain('Kết nối');
    i18n.global.locale.value = 'en';
  });
});
