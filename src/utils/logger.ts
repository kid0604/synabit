import { info, warn, error, debug, trace } from '@tauri-apps/plugin-log';

/**
 * Universal logger wrapper for the frontend.
 * This abstracts away the tauri-plugin-log calls so we can easily swap or augment it later.
 */

export const logger = {
  info: (message: string, ...args: any[]) => {
    const formatted = args.length ? `${message} ${JSON.stringify(args)}` : message;
    info(formatted).catch(console.error);
    console.log(message, ...args); // Fallback to console for devtools visibility
  },
  warn: (message: string, ...args: any[]) => {
    const formatted = args.length ? `${message} ${JSON.stringify(args)}` : message;
    warn(formatted).catch(console.error);
    console.warn(message, ...args);
  },
  error: (message: string, ...args: any[]) => {
    const formatted = args.length ? `${message} ${JSON.stringify(args)}` : message;
    error(formatted).catch(console.error);
    console.error(message, ...args);
  },
  debug: (message: string, ...args: any[]) => {
    const formatted = args.length ? `${message} ${JSON.stringify(args)}` : message;
    debug(formatted).catch(console.error);
    console.debug(message, ...args);
  },
  trace: (message: string, ...args: any[]) => {
    const formatted = args.length ? `${message} ${JSON.stringify(args)}` : message;
    trace(formatted).catch(console.error);
    console.trace(message, ...args);
  }
};
