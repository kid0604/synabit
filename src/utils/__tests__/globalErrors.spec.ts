import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createApp, defineComponent, h } from 'vue';

const logged = vi.hoisted(() => ({ error: [] as unknown[][], warn: [] as unknown[][] }));
vi.mock('../logger', () => ({
  logger: {
    error: (...args: unknown[]) => logged.error.push(args),
    warn: (...args: unknown[]) => logged.warn.push(args),
    info: () => {},
    debug: () => {},
    trace: () => {},
  },
}));

import { installGlobalErrorLogging } from '../globalErrors';

beforeEach(() => {
  logged.error.length = 0;
  logged.warn.length = 0;
});

describe('uncaught errors reach the log', () => {
  it('logs a throw inside a component once, with the root it came from', () => {
    const app = createApp(defineComponent({
      setup() {
        throw new Error('setup broke');
      },
      render: () => h('div'),
    }));
    const uninstall = installGlobalErrorLogging(app, 'main');
    app.mount(document.createElement('div'));
    uninstall();

    expect(logged.error).toHaveLength(1);
    expect(String(logged.error[0][0])).toContain('[main]');
    expect((logged.error[0][1] as Error).message).toBe('setup broke');
    app.unmount();
  });

  it('logs a rejection nobody handled, and a run of the same one only once', () => {
    const app = createApp({ render: () => null });
    const uninstall = installGlobalErrorLogging(app, 'quick-entry');
    const reject = (reason: unknown) => {
      const event = new Event('unhandledrejection') as Event & { reason: unknown };
      event.reason = reason;
      window.dispatchEvent(event);
    };

    reject({ code: 'io', message: 'disk full' });
    reject({ code: 'io', message: 'disk full' });
    reject({ code: 'io', message: 'disk full' });
    reject('something else');

    expect(logged.error).toHaveLength(2);
    expect(String(logged.error[0][0])).toContain('[quick-entry]');
    expect(logged.warn[0][0]).toContain('repeated 2 more time');
    uninstall();
  });
});
