"""Keep one HUD per user: a second launch asks the running one to show itself and exits.

Without this, double-clicking the exe again opened a second window and tray icon, and the second copy's global
hotkey registration failed with a warning. The first copy listens on a local socket (a named pipe on Windows);
a later launch connects, sends WAKE and leaves.

If anything goes wrong opening the socket the app starts anyway: a missing guard is better than no HUD.
"""
import getpass
import os

from PySide6.QtCore import QObject, Signal
from PySide6.QtNetwork import QLocalServer, QLocalSocket

from core.logger import logger

CONNECT_TIMEOUT_MS = 500
WRITE_TIMEOUT_MS = 500
WAKE = b"WAKE\n"


NAME_ENV = "CLAUDEHUD_SINGLE_INSTANCE_NAME"   # lets tests run a real HUD beside the one on the desk


def default_name() -> str:
    """Per user, so a second Windows session does not block the first; not the Rust build's lock name."""
    if os.environ.get(NAME_ENV):
        return os.environ[NAME_ENV]
    try:
        user = getpass.getuser()
    except Exception:
        user = "user"
    safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in user) or "user"
    return f"ClaudeHUDMonitor-Python-{safe}"


class SingleInstance(QObject):
    wakeRequested = Signal()

    def __init__(self, name: str = None, parent=None):
        super().__init__(parent)
        self.name = name or default_name()
        self._server = None

    def is_primary(self) -> bool:
        return self._server is not None

    def acquire(self) -> bool:
        """True when this process is the one to keep running; False after waking the one that already is."""
        if self._wake_existing():
            return False
        self._listen()
        return True

    def release(self):
        if self._server is not None:
            self._server.close()
            self._server.deleteLater()
            self._server = None

    def _wake_existing(self) -> bool:
        socket = QLocalSocket()
        socket.connectToServer(self.name)
        if not socket.waitForConnected(CONNECT_TIMEOUT_MS):
            return False
        socket.write(WAKE)
        socket.flush()
        socket.waitForBytesWritten(WRITE_TIMEOUT_MS)
        socket.disconnectFromServer()
        if socket.state() != QLocalSocket.LocalSocketState.UnconnectedState:
            socket.waitForDisconnected(WRITE_TIMEOUT_MS)
        return True

    def _listen(self):
        QLocalServer.removeServer(self.name)          # a crashed run leaves its socket file behind on macOS/Linux
        server = QLocalServer(self)
        server.setSocketOptions(QLocalServer.SocketOption.UserAccessOption)
        if not server.listen(self.name):
            logger.warning(f"[SingleInstance] Cannot listen on '{self.name}': {server.errorString()}; "
                           "starting without the guard")
            server.deleteLater()
            return
        server.newConnection.connect(self._on_connection)
        self._server = server

    def _on_connection(self):
        while self._server is not None and self._server.hasPendingConnections():
            client = self._server.nextPendingConnection()
            # Qt's own slot cleans the socket up (it stays a child of the server, so it cannot outlive it)
            client.disconnected.connect(client.deleteLater)
            client.readyRead.connect(self._on_data)
            if client.bytesAvailable():
                self._handle(client)

    def _on_data(self):
        client = self.sender()
        if client is not None:
            self._handle(client)

    def _handle(self, client):
        data = bytes(client.readAll())
        if WAKE.strip() in data:
            logger.info("[SingleInstance] Another launch asked this one to show itself")
            self.wakeRequested.emit()
        client.disconnectFromServer()
