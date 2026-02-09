/** WebSocket client with auto-reconnect for HIVE Commander UI. */

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected';

export interface ConnectionOptions {
  url: string;
  onMessage: (data: unknown) => void;
  onStatusChange: (status: ConnectionStatus) => void;
  onError?: (error: string) => void;
  reconnectInterval?: number;
  maxReconnectInterval?: number;
}

export class ViewerConnection {
  private ws: WebSocket | null = null;
  private opts: Required<ConnectionOptions>;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private currentInterval: number;
  private closed = false;

  constructor(opts: ConnectionOptions) {
    this.opts = {
      reconnectInterval: 1000,
      maxReconnectInterval: 10000,
      onError: () => {},
      ...opts,
    };
    this.currentInterval = this.opts.reconnectInterval;
  }

  connect(): void {
    this.closed = false;
    this.doConnect();
  }

  disconnect(): void {
    this.closed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    this.opts.onStatusChange('disconnected');
  }

  private doConnect(): void {
    if (this.closed) return;
    this.opts.onStatusChange('connecting');

    const ws = new WebSocket(this.opts.url);
    this.ws = ws;

    ws.onopen = () => {
      this.currentInterval = this.opts.reconnectInterval;
      this.opts.onStatusChange('connected');
    };

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this.opts.onMessage(data);
      } catch {
        console.warn('[Commander] Failed to parse WebSocket message:', event.data);
      }
    };

    ws.onclose = () => {
      this.ws = null;
      if (!this.closed) {
        this.opts.onStatusChange('disconnected');
        this.scheduleReconnect();
      }
    };

    ws.onerror = (ev) => {
      console.error('[Commander] WebSocket error:', this.opts.url, ev);
      this.opts.onError(`WebSocket error connecting to ${this.opts.url}`);
    };
  }

  private scheduleReconnect(): void {
    if (this.closed) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.currentInterval = Math.min(
        this.currentInterval * 1.5,
        this.opts.maxReconnectInterval,
      );
      this.doConnect();
    }, this.currentInterval);
  }
}

/** Build WebSocket URL from current page location (uses Vite proxy). */
export function defaultWsUrl(): string {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${protocol}//${window.location.host}/ws`;
}
