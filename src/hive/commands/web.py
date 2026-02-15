"""hive web - Mobile-friendly web UI for tmux navigation."""

import asyncio
import json
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Annotated

import typer

SESSION_NAME = "hive-planner"

# HTML template with xterm.js
HTML_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no">
    <title>Hive</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.css">
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }

        :root {
            --bg-primary: #0c0c0c;
            --bg-secondary: #161616;
            --bg-tertiary: #1c1c1c;
            --bg-hover: #252525;
            --border: #2a2a2a;
            --text-primary: #fafafa;
            --text-secondary: #a0a0a0;
            --text-muted: #606060;
            --accent: #f97316;
            --accent-hover: #ea580c;
            --accent-glow: rgba(249, 115, 22, 0.15);
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            min-height: 100vh;
            min-height: 100dvh;
            -webkit-font-smoothing: antialiased;
        }

        /* List view */
        #list-view {
            padding: 24px 16px;
            padding-bottom: 100px;
            max-width: 500px;
            margin: 0 auto;
        }
        #list-view.hidden { display: none; }

        h1 {
            font-size: 28px;
            font-weight: 700;
            margin-bottom: 8px;
            color: var(--text-primary);
            letter-spacing: -0.5px;
        }

        .subtitle {
            font-size: 14px;
            color: var(--text-muted);
            margin-bottom: 32px;
        }

        .pane-card {
            background: var(--bg-secondary);
            border-radius: 16px;
            padding: 16px 20px;
            margin-bottom: 10px;
            cursor: pointer;
            transition: all 0.2s ease;
            border: 1px solid var(--border);
            display: flex;
            align-items: center;
            justify-content: space-between;
        }
        .pane-card:hover {
            background: var(--bg-hover);
            border-color: #3a3a3a;
        }
        .pane-card:active {
            transform: scale(0.98);
        }
        .pane-name {
            font-size: 16px;
            font-weight: 600;
            color: var(--text-primary);
        }
        .pane-info {
            font-size: 13px;
            color: var(--text-muted);
            font-family: 'SF Mono', Menlo, monospace;
        }

        .section-title {
            font-size: 11px;
            font-weight: 600;
            text-transform: uppercase;
            color: var(--text-secondary);
            margin: 28px 0 12px;
            letter-spacing: 0.5px;
            padding-left: 4px;
        }

        .empty-state {
            color: var(--text-muted);
            padding: 40px 20px;
            text-align: center;
            font-size: 15px;
        }

        /* Terminal view */
        #terminal-view {
            display: none;
            height: 100vh;
            height: 100dvh;
            flex-direction: column;
            background: #000;
        }
        #terminal-view.active {
            display: flex;
        }

        #terminal-header {
            background: var(--bg-secondary);
            padding: 12px 16px;
            display: flex;
            align-items: center;
            gap: 12px;
            border-bottom: 1px solid var(--border);
            flex-shrink: 0;
        }

        #back-btn {
            background: transparent;
            color: var(--accent);
            border: none;
            padding: 8px 0;
            font-size: 16px;
            font-weight: 500;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 4px;
        }
        #back-btn:active {
            opacity: 0.7;
        }

        #current-pane {
            font-size: 15px;
            font-weight: 600;
            flex: 1;
            text-align: center;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            color: var(--text-primary);
        }

        #terminal-container {
            flex: 1;
            background: #000;
            overflow: auto;
            -webkit-overflow-scrolling: touch;
        }

        #quick-keys {
            background: var(--bg-secondary);
            padding: 10px 12px;
            display: flex;
            gap: 6px;
            overflow-x: auto;
            border-top: 1px solid var(--border);
            flex-shrink: 0;
            -webkit-overflow-scrolling: touch;
        }

        .quick-key {
            background: var(--bg-tertiary);
            color: var(--text-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 8px 14px;
            font-size: 13px;
            font-family: 'SF Mono', Menlo, monospace;
            cursor: pointer;
            white-space: nowrap;
            transition: all 0.15s ease;
        }
        .quick-key:active {
            background: var(--bg-hover);
            transform: scale(0.95);
        }

        #input-bar {
            background: var(--bg-secondary);
            padding: 12px;
            display: flex;
            gap: 10px;
            flex-shrink: 0;
            border-top: 1px solid var(--border);
        }

        #input-text {
            flex: 1;
            background: var(--bg-primary);
            border: 1px solid var(--border);
            border-radius: 12px;
            padding: 14px 16px;
            color: var(--text-primary);
            font-size: 16px;
            font-family: 'SF Mono', Menlo, monospace;
            outline: none;
            transition: border-color 0.2s;
        }
        #input-text:focus {
            border-color: var(--accent);
            box-shadow: 0 0 0 3px var(--accent-glow);
        }
        #input-text::placeholder {
            color: var(--text-muted);
        }

        #send-btn {
            background: var(--accent);
            color: white;
            border: none;
            border-radius: 12px;
            padding: 14px 24px;
            font-size: 15px;
            font-weight: 600;
            cursor: pointer;
            transition: all 0.15s ease;
        }
        #send-btn:active {
            background: var(--accent-hover);
            transform: scale(0.95);
        }

        /* Refresh FAB */
        #refresh-btn {
            position: fixed;
            bottom: 24px;
            right: 24px;
            background: var(--bg-secondary);
            color: var(--text-primary);
            border: 1px solid var(--border);
            border-radius: 50%;
            width: 56px;
            height: 56px;
            font-size: 22px;
            cursor: pointer;
            box-shadow: 0 4px 20px rgba(0,0,0,0.4);
            transition: all 0.2s ease;
        }
        #refresh-btn:active {
            transform: scale(0.9);
            background: var(--bg-hover);
        }

        .status-msg {
            position: fixed;
            bottom: 100px;
            left: 50%;
            transform: translateX(-50%);
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            padding: 12px 24px;
            border-radius: 100px;
            font-size: 14px;
            font-weight: 500;
            opacity: 0;
            transition: opacity 0.3s;
            box-shadow: 0 4px 20px rgba(0,0,0,0.3);
        }
        .status-msg.show { opacity: 1; }

        /* Scrollbar styling */
        ::-webkit-scrollbar {
            width: 6px;
            height: 6px;
        }
        ::-webkit-scrollbar-track {
            background: transparent;
        }
        ::-webkit-scrollbar-thumb {
            background: var(--border);
            border-radius: 3px;
        }
    </style>
</head>
<body>
    <div id="list-view">
        <h1>Hive</h1>
        <p class="subtitle">Select a pane to connect</p>
        <div id="panes-list"></div>
        <button id="refresh-btn" onclick="loadPanes()">↻</button>
        <div id="status-msg" class="status-msg"></div>
    </div>

    <div id="terminal-view">
        <div id="terminal-header">
            <button id="back-btn" onclick="goBack()">‹ Back</button>
            <span id="current-pane"></span>
            <div style="width: 50px;"></div>
        </div>
        <div id="terminal-container"></div>
        <div id="quick-keys">
            <button class="quick-key" onclick="refreshPane()">🔄</button>
            <button class="quick-key" onclick="sendSpecial('Enter')">↵ Enter</button>
            <button class="quick-key" onclick="sendSpecial('C-c')">^C</button>
            <button class="quick-key" onclick="sendSpecial('C-d')">^D</button>
            <button class="quick-key" onclick="sendSpecial('Tab')">Tab</button>
            <button class="quick-key" onclick="sendSpecial('Escape')">Esc</button>
            <button class="quick-key" onclick="sendSpecial('Up')">↑</button>
            <button class="quick-key" onclick="sendSpecial('Down')">↓</button>
        </div>
        <div id="input-bar">
            <input type="text" id="input-text" placeholder="Type here..." autocomplete="off" autocapitalize="off" autocorrect="off">
            <button id="send-btn" onclick="sendInput()">Send</button>
        </div>
    </div>

    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.min.js"></script>
    <script>
        let term = null;
        let ws = null;
        let fitAddon = null;
        let currentPane = null;

        function showStatus(msg) {
            const el = document.getElementById('status-msg');
            el.textContent = msg;
            el.classList.add('show');
            setTimeout(() => el.classList.remove('show'), 2000);
        }

        async function loadPanes() {
            try {
                const resp = await fetch('/api/panes');
                const data = await resp.json();
                renderPanes(data.panes);
                showStatus('Refreshed');
            } catch (e) {
                showStatus('Error loading panes');
            }
        }

        function renderPanes(panes) {
            const container = document.getElementById('panes-list');

            // Sort panes by window_index, then pane index (same order as tmux)
            panes.sort((a, b) => {
                if (a.window_index !== b.window_index) {
                    return a.window_index - b.window_index;
                }
                return a.index - b.index;
            });

            // Group by window (maintaining order)
            const byWindow = [];
            const windowMap = {};
            panes.forEach(p => {
                if (!(p.window in windowMap)) {
                    windowMap[p.window] = byWindow.length;
                    byWindow.push({ name: p.window, panes: [] });
                }
                byWindow[windowMap[p.window]].panes.push(p);
            });

            let html = '';

            for (const win of byWindow) {
                html += `<div class="section-title">${win.name}</div>`;

                win.panes.forEach(p => {
                    const label = p.label || p.cmd || `pane ${p.index}`;
                    html += `
                        <div class="pane-card" onclick="connectTo('${p.id}', '${win.name}', '${label}')">
                            <div class="pane-name">${label}</div>
                            <div class="pane-info">${p.width}x${p.height}</div>
                        </div>
                    `;
                });
            }

            if (panes.length === 0) {
                html = '<div class="empty-state">No panes found. Is hive-planner running?</div>';
            }

            container.innerHTML = html;
        }

        function connectTo(paneId, windowName, label, skipHistory) {
            currentPane = paneId;
            document.getElementById('current-pane').textContent = `${windowName} / ${label}`;
            document.getElementById('list-view').classList.add('hidden');
            document.getElementById('terminal-view').classList.add('active');

            // Update URL for back button support
            if (!skipHistory) {
                history.pushState(
                    { paneId, windowName, label },
                    '',
                    `#${encodeURIComponent(paneId)}`
                );
            }

            // Create terminal
            if (term) {
                term.dispose();
            }

            term = new Terminal({
                fontSize: 14,
                fontFamily: 'Menlo, Monaco, monospace',
                theme: {
                    background: '#000000',
                    foreground: '#ffffff',
                },
                cursorBlink: false,
                disableStdin: true,  // Read-only display
                scrollback: 5000,
                convertEol: true,
            });

            fitAddon = new FitAddon.FitAddon();
            term.loadAddon(fitAddon);

            const container = document.getElementById('terminal-container');
            container.innerHTML = '';
            term.open(container);

            // Fit after a brief delay
            setTimeout(() => {
                fitAddon.fit();
                connectWebSocket(paneId);
            }, 50);

            // Handle resize
            const resizeHandler = () => {
                if (fitAddon && term) {
                    fitAddon.fit();
                    sendResize();
                }
            };
            window.addEventListener('resize', resizeHandler);
            term._resizeHandler = resizeHandler;

            // Focus input and setup enter key
            const inputEl = document.getElementById('input-text');
            inputEl.focus();
            inputEl.addEventListener('keydown', (e) => {
                if (e.key === 'Enter') {
                    e.preventDefault();
                    sendInput();
                }
            });
        }

        function sendInput() {
            const inputEl = document.getElementById('input-text');
            const text = inputEl.value;
            if (ws && ws.readyState === WebSocket.OPEN && text) {
                ws.send(JSON.stringify({ type: 'input', data: text }));
                ws.send(JSON.stringify({ type: 'special', key: 'Enter' }));
                inputEl.value = '';
            }
            inputEl.focus();
        }

        function sendSpecial(key) {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'special', key: key }));
            }
            document.getElementById('input-text').focus();
        }

        function refreshPane() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'refresh' }));
            }
        }

        function connectWebSocket(paneId) {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            // Encode pane ID (it has % in it)
            ws = new WebSocket(`${protocol}//${window.location.host}/ws/pane/${encodeURIComponent(paneId)}`);

            ws.onopen = () => {
                // Request initial content and set size
                sendResize();
            };

            ws.onmessage = (event) => {
                const msg = JSON.parse(event.data);
                if (msg.type === 'output') {
                    term.write(msg.data);
                } else if (msg.type === 'clear') {
                    term.clear();
                }
            };

            ws.onclose = () => {
                term.write('\\r\\n[Disconnected]\\r\\n');
            };

            ws.onerror = () => {
                term.write('\\r\\n[Connection error]\\r\\n');
            };
        }

        function sendResize() {
            if (ws && ws.readyState === WebSocket.OPEN && term) {
                ws.send(JSON.stringify({
                    type: 'resize',
                    cols: term.cols,
                    rows: term.rows
                }));
            }
        }

        function goBack() {
            if (ws) {
                ws.close();
                ws = null;
            }
            if (term && term._resizeHandler) {
                window.removeEventListener('resize', term._resizeHandler);
            }
            document.getElementById('terminal-view').classList.remove('active');
            document.getElementById('list-view').classList.remove('hidden');
            currentPane = null;

            // Update URL
            history.pushState({}, '', '/');
            loadPanes();
        }

        // Handle browser back/forward buttons
        window.addEventListener('popstate', (event) => {
            if (event.state && event.state.paneId) {
                // Going forward to a pane
                connectTo(event.state.paneId, event.state.windowName, event.state.label, true);
            } else {
                // Going back to list
                if (ws) {
                    ws.close();
                    ws = null;
                }
                if (term && term._resizeHandler) {
                    window.removeEventListener('resize', term._resizeHandler);
                }
                document.getElementById('terminal-view').classList.remove('active');
                document.getElementById('list-view').classList.remove('hidden');
                currentPane = null;
                loadPanes();
            }
        });

        // Initial load - check for hash in URL
        async function init() {
            await loadPanes();

            // If URL has a pane hash, connect to it
            if (window.location.hash) {
                const paneId = decodeURIComponent(window.location.hash.slice(1));
                // Find pane info from loaded panes
                const resp = await fetch('/api/panes');
                const data = await resp.json();
                const pane = data.panes.find(p => p.id === paneId);
                if (pane) {
                    connectTo(pane.id, pane.window, pane.label, true);
                }
            }
        }

        init();
    </script>
</body>
</html>
"""


def _get_panes() -> list[dict]:
    """Get all tmux panes for the hive session."""
    try:
        result = subprocess.run(
            [
                "tmux", "list-panes", "-s", "-t", SESSION_NAME,
                "-F", "#{pane_id}\t#{window_name}\t#{window_index}\t#{pane_index}\t#{pane_title}\t#{pane_current_command}\t#{pane_width}\t#{pane_height}\t#{window_panes}"
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            return []

        panes = []
        for line in result.stdout.strip().split("\n"):
            if not line:
                continue
            parts = line.split("\t")
            if len(parts) >= 9:
                window = parts[1]
                window_index = int(parts[2])
                pane_index = int(parts[3])
                title = parts[4]
                cmd = parts[5]
                total_panes = int(parts[8])

                # Check if title is a real custom title (not hostname or default)
                is_custom_title = (
                    title and
                    title not in ("", "zsh", "bash", "sh", cmd) and
                    " " not in title and  # Hostnames often have spaces like "Josh's Mac"
                    not title.endswith("-mini") and
                    not title.endswith("-pro") and
                    not title.startswith("ttys")
                )

                if is_custom_title:
                    label = title
                elif cmd == "claude" or "claude" in cmd.lower():
                    label = "claude"
                elif window not in ("planner", "shell") and total_panes == 2:
                    # Worker window with 2 panes: first is claude, second is shell
                    label = "claude" if pane_index == 0 else "shell"
                else:
                    label = cmd or f"pane {pane_index}"

                panes.append({
                    "id": parts[0],
                    "window": window,
                    "window_index": window_index,
                    "index": pane_index,
                    "label": label,
                    "width": int(parts[6]),
                    "height": int(parts[7]),
                })
        return panes
    except Exception:
        return []


def web(
    ctx: typer.Context,
    port: Annotated[
        int,
        typer.Option("--port", "-p", help="Port to run on."),
    ] = 8080,
    host: Annotated[
        str,
        typer.Option("--host", "-h", help="Host to bind to."),
    ] = "0.0.0.0",
) -> None:
    """Start mobile-friendly web UI for hive.

    Opens a web interface for navigating tmux panes.
    Access from your phone at http://<your-ip>:8080
    """
    try:
        from aiohttp import web as aiohttp_web
    except ImportError:
        typer.secho("Missing dependency. Install with:", fg="yellow")
        typer.echo("  pip install aiohttp")
        raise typer.Exit(1)

    import aiohttp

    async def handle_index(request):
        return aiohttp_web.Response(text=HTML_TEMPLATE, content_type="text/html")

    async def handle_panes(request):
        panes = _get_panes()
        return aiohttp_web.json_response({"panes": panes})

    # Create temp directory for pipe files
    pipe_dir = Path(tempfile.gettempdir()) / "hive-web-pipes"
    pipe_dir.mkdir(exist_ok=True)

    async def handle_websocket(request):
        pane_id = request.match_info["pane_id"]

        ws = aiohttp_web.WebSocketResponse()
        await ws.prepare(request)

        # Create pipe file for this connection
        pipe_file = pipe_dir / f"{pane_id.replace('%', 'p')}-{id(ws)}.pipe"
        tail_process = None
        running = True

        def send_to_pane(keys: str):
            """Send literal text to the pane."""
            subprocess.run(
                ["tmux", "send-keys", "-t", pane_id, "-l", "--", keys],
                capture_output=True,
            )

        def send_special_key(key: str):
            """Send special key to the pane."""
            subprocess.run(
                ["tmux", "send-keys", "-t", pane_id, key],
                capture_output=True,
            )

        def get_pane_size():
            """Get current pane dimensions."""
            result = subprocess.run(
                ["tmux", "display-message", "-t", pane_id, "-p", "#{pane_width}\t#{pane_height}"],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                parts = result.stdout.strip().split("\t")
                if len(parts) == 2:
                    return int(parts[0]), int(parts[1])
            return None, None

        def resize_pane(cols: int, rows: int):
            """Resize the tmux pane."""
            subprocess.run(
                ["tmux", "resize-pane", "-t", pane_id, "-x", str(cols), "-y", str(rows)],
                capture_output=True,
            )

        # Save original dimensions to restore later
        original_cols, original_rows = get_pane_size()

        def start_pipe():
            """Start piping pane output to file."""
            # Clear any existing pipe
            subprocess.run(
                ["tmux", "pipe-pane", "-t", pane_id],
                capture_output=True,
            )
            # Start new pipe
            pipe_file.touch()
            subprocess.run(
                ["tmux", "pipe-pane", "-t", pane_id, f"cat >> {pipe_file}"],
                capture_output=True,
            )

        def stop_pipe():
            """Stop piping pane output."""
            subprocess.run(
                ["tmux", "pipe-pane", "-t", pane_id],
                capture_output=True,
            )

        def capture_initial():
            """Capture current pane content for initial display."""
            result = subprocess.run(
                ["tmux", "capture-pane", "-t", pane_id, "-p", "-e"],
                capture_output=True,
                text=True,
            )
            return result.stdout if result.returncode == 0 else ""

        async def tail_pipe():
            """Tail the pipe file and stream to websocket."""
            nonlocal tail_process, running
            try:
                # Start tail process
                tail_process = await asyncio.create_subprocess_exec(
                    "tail", "-n", "0", "-F", str(pipe_file),
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.DEVNULL,
                )

                while running and tail_process.stdout:
                    try:
                        data = await asyncio.wait_for(
                            tail_process.stdout.read(4096),
                            timeout=0.5
                        )
                        if data:
                            text = data.decode("utf-8", errors="replace")
                            await ws.send_json({"type": "output", "data": text})
                    except asyncio.TimeoutError:
                        continue
                    except Exception:
                        break
            except asyncio.CancelledError:
                pass
            finally:
                if tail_process:
                    try:
                        tail_process.terminate()
                        await tail_process.wait()
                    except Exception:
                        pass

        # Setup: start pipe and send initial content
        start_pipe()
        initial_content = capture_initial()
        if initial_content:
            await ws.send_json({"type": "output", "data": initial_content})

        # Start tailing in background
        tail_task = asyncio.create_task(tail_pipe())

        try:
            async for msg in ws:
                if msg.type == aiohttp.WSMsgType.TEXT:
                    data = json.loads(msg.data)

                    if data["type"] == "input":
                        text = data.get("data", "")
                        if text:
                            send_to_pane(text)

                    elif data["type"] == "special":
                        key = data.get("key", "")
                        if key:
                            send_special_key(key)

                    elif data["type"] == "refresh":
                        # Re-capture full pane content
                        content = capture_initial()
                        if content:
                            await ws.send_json({"type": "output", "data": "\033[2J\033[H" + content})

                    elif data["type"] == "resize":
                        cols = data.get("cols", 80)
                        rows = data.get("rows", 24)
                        resize_pane(cols, rows)

                elif msg.type == aiohttp.WSMsgType.ERROR:
                    break
        finally:
            # Cleanup
            running = False
            stop_pipe()
            tail_task.cancel()
            try:
                await tail_task
            except asyncio.CancelledError:
                pass
            try:
                pipe_file.unlink(missing_ok=True)
            except Exception:
                pass

            # Restore original pane dimensions
            if original_cols and original_rows:
                resize_pane(original_cols, original_rows)

        return ws

    # Create app
    app = aiohttp_web.Application()
    app.router.add_get("/", handle_index)
    app.router.add_get("/api/panes", handle_panes)
    app.router.add_get("/ws/pane/{pane_id}", handle_websocket)

    # Get local IP for mobile access
    import socket
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        local_ip = s.getsockname()[0]
        s.close()
    except Exception:
        local_ip = "localhost"

    typer.echo()
    typer.secho("🐝 Hive Web UI", fg="green", bold=True)
    typer.echo()
    typer.echo(f"  Local:   http://localhost:{port}")
    typer.echo(f"  Mobile:  http://{local_ip}:{port}")
    typer.echo()
    typer.secho("  Press Ctrl+C to stop", fg="bright_black")
    typer.echo()

    aiohttp_web.run_app(app, host=host, port=port, print=None)
