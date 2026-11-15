// SPDX-License-Identifier: FSL-1.1-MIT
// Constants shared by the service worker and the popup. The port must match LINK_PORT in the app.
export const PORT = 47815;
export const WS_URL = "ws://127.0.0.1:47815/";
export const MAX_MSG = 4096;
export const PING_MS = 20000;
export const ALARM = "so-reconnect";
export const ALARM_MIN = 0.5;
