import { info, warn, error, debug, trace } from '@tauri-apps/plugin-log';

/**
 * Universal logger wrapper for the frontend.
 * Implements a queue to prevent WebKit "Fetch API" CORS errors when IPC is called while the app is backgrounded.
 */

type LogLevel = 'info' | 'warn' | 'error' | 'debug' | 'trace';
interface QueuedLog { level: LogLevel; message: string; }

let logQueue: QueuedLog[] = [];

// Flush queue when app returns to foreground
if (typeof document !== 'undefined') {
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && logQueue.length > 0) {
      const queue = [...logQueue];
      logQueue = [];
      for (const log of queue) {
        dispatchIpcLog(log.level, log.message);
      }
    }
  });
}

function dispatchIpcLog(level: LogLevel, message: string) {
  const promise = 
    level === 'info' ? info(message) :
    level === 'warn' ? warn(message) :
    level === 'error' ? error(message) :
    level === 'debug' ? debug(message) :
    trace(message);
    
  promise.catch((e) => {
    // Ignore access control errors caused by WebKit backgrounding edge cases
    if (e && e.toString().includes('access control checks')) return;
    console.error('[Logger IPC Error]', e);
  });
}

/**
 * Render anything as a string for the log IPC, which only accepts strings.
 *
 * Callers are typed to pass a string, but `.catch(logger.error)` hands the
 * rejection value straight into the message position, so in practice it is
 * often an Error or a plain object. Those calls used to be rejected by the
 * backend with "invalid type: map, expected a string" — losing exactly the
 * failures worth reading.
 */
function toMessage(value: unknown): string {
  if (typeof value === 'string') return value;
  if (value instanceof Error) {
    return value.stack ? `${value.name}: ${value.message}\n${value.stack}` : `${value.name}: ${value.message}`;
  }
  if (value === undefined) return 'undefined';
  if (value === null) return 'null';
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    // Circular structures, and anything else JSON cannot render.
    return String(value);
  }
}

function safeLog(level: LogLevel, message: unknown, ...args: any[]) {
  const head = toMessage(message);
  const formatted = args.length ? `${head} ${args.map(toMessage).join(' ')}` : head;

  // Console log immediately for DevTools
  if (level === 'info') console.log(message, ...args);
  else if (level === 'warn') console.warn(message, ...args);
  else if (level === 'error') console.error(message, ...args);
  else if (level === 'debug') console.debug(message, ...args);
  else console.trace(message, ...args);

  // Defer IPC if backgrounded to avoid Fetch cancellation error
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
    logQueue.push({ level, message: formatted });
  } else {
    dispatchIpcLog(level, formatted);
  }
}

// `unknown` rather than `string` because `.catch(logger.error)` is used widely
// and hands a rejection value into the first position.
export const logger = {
  info: (message: unknown, ...args: any[]) => safeLog('info', message, ...args),
  warn: (message: unknown, ...args: any[]) => safeLog('warn', message, ...args),
  error: (message: unknown, ...args: any[]) => safeLog('error', message, ...args),
  debug: (message: unknown, ...args: any[]) => safeLog('debug', message, ...args),
  trace: (message: unknown, ...args: any[]) => safeLog('trace', message, ...args),
};
