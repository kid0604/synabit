import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import { commandText, sends, type DeclaredView } from '../../declared';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

import ConnectCard from '../ConnectCard.vue';

const FP = 'SHA256:abcDEF123ghiJKL456mnoPQR789stuVWX012yzA345';

const pi: DeclaredView = {
  node_id: 'SynConnectors/Raspberry Pi.md',
  title: 'Raspberry Pi',
  manifest: {
    id: 'pi',
    name: 'Raspberry Pi',
    version: 1,
    provenance: 'syn',
    transport: 'ssh',
    base_url: '{{input.host}}',
    auth: { type: 'ssh', secret: 'login' },
    inputs: [
      { name: 'host', kind: 'address', label: { en: 'Address', vi: 'Địa chỉ' }, example: 'ssh://pi@raspberrypi.local' },
      { name: 'login', kind: 'secret', label: { en: 'Password or private key', vi: 'Mật khẩu hoặc khoá riêng' } },
    ],
    operations: [
      { name: 'disk', description: 'Disk use.', effect: 'read_attested', command: ['df', '-h', '/'] },
      { name: 'restart', description: 'Restart a service.', effect: 'write', command: ['sudo', 'systemctl', 'restart', '{{unit}}'] },
    ],
  },
  problems: [],
  standing: { state: 'new' },
  instance: null,
  effects: {},
  secret_here: false,
};

const show = () => mount(ConnectCard, { props: { vaultPath: '/v', connectorId: 'pi' }, global: { plugins: [i18n] } });

describe('Connecting to a machine over SSH', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
  });

  it('shows every command Syn may run, and that each asks first', async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_manifest_list' ? [pi] : null));
    const w = show();
    await flushPromises();
    await w.find('[data-test="input-host"] input').setValue('ssh://pi@raspberrypi.local');
    expect(w.find('[data-test="connect-what"]').text()).toContain('run commands on raspberrypi.local');
    expect(w.find('[data-test="connect-what"]').text()).toContain('asks you before it runs');
    const commands = w.find('[data-test="connect-commands"]').text();
    expect(commands).toContain('df -h /');
    expect(commands).toContain('sudo systemctl restart <unit>');
    expect(commands).toContain('cannot run anything else');
    expect(w.find('[data-test="input-login"] input').attributes('type')).toBe('password');
  });

  it('meets the machine: shows its key and pins it on "This is my machine"', async () => {
    const answers = [
      { ok: false, code: 'host_key_unknown', cert: FP, said: 'not met yet' },
      { ok: true, said: '' },
    ];
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_manifest_list') return [pi];
      if (cmd === 'syn_connect') return answers.shift();
      return null;
    });
    const w = show();
    await flushPromises();
    await w.find('[data-test="input-host"] input').setValue('ssh://pi@raspberrypi.local');
    await w.find('[data-test="input-login"] input').setValue('raspberry');
    await w.find('[data-test="connect"]').trigger('click');
    await flushPromises();
    expect(w.find('[data-test="connect-problem"]').text()).toContain('First time connecting');
    expect(w.find('[data-test="host-key"]').text()).toBe(FP);
    expect(w.find('[data-test="trust"]').text()).toBe('This is my machine');

    await w.find('[data-test="trust"]').trigger('click');
    await flushPromises();
    const calls = invoke.mock.calls.filter(([c]) => c === 'syn_connect');
    expect(calls[0][1]).toMatchObject({ secrets: { login: 'raspberry' }, trustCert: null });
    expect(calls[1][1]).toMatchObject({ trustCert: FP });
    expect(w.emitted('done')).toEqual([[true]]);
  });

  it('a changed key is a warning, not a welcome', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_manifest_list') return [pi];
      if (cmd === 'syn_connect') return { ok: false, code: 'host_key_changed', cert: FP, said: 'changed' };
      return null;
    });
    const w = show();
    await flushPromises();
    await w.find('[data-test="connect"]').trigger('click');
    await flushPromises();
    expect(w.find('[data-test="connect-problem"]').text()).toContain('different key');
    expect(w.find('[data-test="trust"]').text()).toBe('Trust the new key');
  });

  it('every ssh command is a send until vouched for, and a command reads as words', () => {
    expect(sends(pi.manifest!)).toBe(true);
    expect(commandText(['echo', '{{text}}', '--tail={{n}}', 'two words'])).toBe("echo <text> --tail=<n> 'two words'");
  });

  describe('approved before, and Syn added a command', () => {
    const changed: DeclaredView = {
      ...pi,
      standing: { state: 'changed' },
      instance: {
        manifest_id: 'pi',
        label: '',
        zone: { kind: 'personal' },
        exposure: 'full',
        network: 'internal',
        pinned_cert: FP,
        attested: [],
        inputs: { host: 'ssh://pi@192.168.1.3' },
        approved: 'old',
        approved_at: '2026-10-13T00:00:00Z',
      },
      secret_here: true,
      not_yet_approved: ['restart'],
    };

    it('asks about the new command, not the password', async () => {
      invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_manifest_list' ? [changed] : null));
      const w = show();
      await flushPromises();
      expect(w.find('[data-test="connect-again"]').text()).toBe('Syn wants to add a command on 192.168.1.3:');
      expect(w.find('[data-test="connect-what"]').exists()).toBe(false);
      expect(w.find('[data-test="input-login"]').exists()).toBe(false);
      expect(w.find('[data-test="input-host"]').exists()).toBe(false);
      expect(w.find('[data-test="connect-kept"]').text()).toContain('Password kept on this device.');
      expect(w.find('[data-test="command-restart"] [data-test="command-new"]').exists()).toBe(true);
      expect(w.find('[data-test="command-disk"]').exists()).toBe(false);
      expect(w.find('[data-test="commands-allowed"]').text()).toContain('df -h /');
      expect(w.find('[data-test="connect"]').text()).toBe('Allow');
    });

    it('allows with what it kept, sending no password', async () => {
      invoke.mockImplementation(async (cmd: string) => {
        if (cmd === 'syn_manifest_list') return [changed];
        if (cmd === 'syn_connect') return { ok: true, said: '' };
        return null;
      });
      const w = show();
      await flushPromises();
      await w.find('[data-test="connect"]').trigger('click');
      await flushPromises();
      const call = invoke.mock.calls.find(([cmd]) => cmd === 'syn_connect')?.[1];
      expect(call.inputs).toEqual({ host: 'ssh://pi@192.168.1.3' });
      expect(call.secrets).toEqual({});
    });

    it('opens the fields on asking', async () => {
      invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_manifest_list' ? [changed] : null));
      const w = show();
      await flushPromises();
      await w.find('[data-test="connect-change"]').trigger('click');
      expect(w.find('[data-test="input-login"] input').attributes('placeholder')).toContain('Kept on this device');
      expect((w.find('[data-test="input-host"] input').element as HTMLInputElement).value).toBe('ssh://pi@192.168.1.3');
    });

    it('shows the fields when the password is not on this device', async () => {
      invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_manifest_list' ? [{ ...changed, secret_here: false }] : null));
      const w = show();
      await flushPromises();
      expect(w.find('[data-test="input-login"]').exists()).toBe(true);
    });
  });
});
