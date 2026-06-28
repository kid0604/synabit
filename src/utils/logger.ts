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

function safeLog(level: LogLevel, message: string, ...args: any[]) {
  const formatted = args.length ? `${message} ${JSON.stringify(args)}` : message;
  
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

export const logger = {
  info: (message: string, ...args: any[]) => safeLog('info', message, ...args),
  warn: (message: string, ...args: any[]) => safeLog('warn', message, ...args),
  error: (message: string, ...args: any[]) => safeLog('error', message, ...args),
  debug: (message: string, ...args: any[]) => safeLog('debug', message, ...args),
  trace: (message: string, ...args: any[]) => safeLog('trace', message, ...args),
};
