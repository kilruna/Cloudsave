#!/usr/bin/python3
"""Native GTK + real session D-Bus lifecycle regression (isolated test bus only).

Run under dbus-run-session and xvfb-run; the panel watcher is a protocol fixture,
while the app, window actions, secondary process, and background sync are real.
"""
import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
import pathlib
from queue import Queue
import sqlite3
import subprocess
import sys
from threading import Event, Thread
import time
from unittest.mock import patch

from gi.repository import Gio, GLib
import run_ui_tests as ui

APP = ui.APP
ITEM = '/StatusNotifierItem'
MENU = '/MenuBar'
SNI = 'org.kde.StatusNotifierItem'
WATCHER = 'org.kde.StatusNotifierWatcher'
DBUSMENU = 'com.canonical.dbusmenu'
NOTIFICATIONS = 'org.freedesktop.Notifications'


def eventually(fn, timeout=10):
    def poll():
        while GLib.MainContext.default().iteration(False):
            pass
        return fn()
    assert ui.eventually(poll, timeout), 'Timed out waiting for lifecycle condition'


def call(connection, path, interface, method, signature=None, values=()):
    return connection.call_sync(APP, path, interface, method,
                                GLib.Variant(signature, values) if signature else None,
                                None, Gio.DBusCallFlags.NONE, 5000, None).unpack()


class Watcher:
    def __init__(self, connection):
        self.connection = connection
        self.registrations = []
        info = Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.kde.StatusNotifierWatcher">
          <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
        </interface></node>''').interfaces[0]
        self.object_id = connection.register_object('/StatusNotifierWatcher', info, self.register, None, None)
        self.own()

    def register(self, connection, sender, path, interface, method, params, invocation):
        self.registrations.append((sender, params.unpack()[0]))
        invocation.return_value(None)

    def own(self):
        self.connection.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                                  'org.freedesktop.DBus', 'RequestName',
                                  GLib.Variant('(su)', (WATCHER, 4)), None,
                                  Gio.DBusCallFlags.NONE, 5000, None)

    def release(self):
        self.connection.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                                  'org.freedesktop.DBus', 'ReleaseName',
                                  GLib.Variant('(s)', (WATCHER,)), None,
                                  Gio.DBusCallFlags.NONE, 5000, None)


class Notifications:
    def __init__(self, connection):
        self.shown = []
        info = Gio.DBusNodeInfo.new_for_xml('''<node><interface name="org.freedesktop.Notifications">
          <method name="Notify">
            <arg type="s" direction="in"/><arg type="u" direction="in"/>
            <arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/>
            <arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/>
            <arg type="u" direction="out"/>
          </method>
          <method name="GetCapabilities"><arg type="as" direction="out"/></method>
          <signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
        </interface></node>''').interfaces[0]
        connection.register_object('/org/freedesktop/Notifications', info, self.handle, None, None)
        connection.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                             'org.freedesktop.DBus', 'RequestName',
                             GLib.Variant('(su)', (NOTIFICATIONS, 4)), None,
                             Gio.DBusCallFlags.NONE, 5000, None)

    def handle(self, connection, sender, path, interface, method, params, invocation):
        if method == 'Notify':
            self.shown.append(params.unpack())
            invocation.return_value(GLib.Variant('(u)', (len(self.shown),)))
        else:
            invocation.return_value(GLib.Variant('(as)', (['body', 'actions'],)))


class SessionUI(ui.UI):
    def launch(self, background=False):
        root = str(self.root)
        env = os.environ.copy()
        env.update(HOME=root+'/home', XDG_CONFIG_HOME=root+'/config',
                   XDG_STATE_HOME=root+'/state', XDG_DATA_HOME=root+'/data',
                   XDG_CACHE_HOME=root+'/cache', CLOUDSAVE_QA_DIR=root+'/control',
                   LD_PRELOAD=str(ui.HERE/'gtk_qa_bridge.so'), GTK_USE_PORTAL='0',
                   NO_AT_BRIDGE='1', GDK_BACKEND='x11')
        self.env = env
        self.logfile = (ui.OUT/(self.name+'.stderr.log')).open('w')
        self.proc = subprocess.Popen([str(ui.BINARY)] + (['--background'] if background else []),
                                     env=env, stdout=self.logfile, stderr=self.logfile,
                                     start_new_session=True)
        self.command('dump', timeout=12)
        actual = pathlib.Path(f'/proc/{self.proc.pid}/environ').read_bytes().split(b'\0')
        assert f'XDG_CONFIG_HOME={root}/config'.encode() in actual
        assert f'HOME={root}/home'.encode() in actual
        self.assert_window(not background)
        return self

    def assert_window(self, visible):
        roots = [n for n in self.tree() if n['type'] == 'GtkApplicationWindow']
        assert len(roots) == 1, f'Duplicate/lost application window: {roots}'
        assert bool(roots[0]['visible']) == visible, roots
        assert self.proc.poll() is None, 'App stopped unexpectedly'

    def close_window(self):
        window = self.find('GtkApplicationWindow')
        self.command('close', window['id'])
        self.assert_window(False)

    def secondary(self, background=False):
        # No test bridge in the secondary process; it must forward to the primary.
        env = self.env.copy()
        env.pop('LD_PRELOAD')
        subprocess.run([str(ui.BINARY)] + (['--background'] if background else []),
                       env=env, check=True, timeout=8, capture_output=True)

    def expect_exit_on_close(self):
        window = self.find('GtkApplicationWindow')
        (self.root/'control/command').write_text(f'999\tclose\t{window["id"]}\t\n')
        assert self.proc.wait(timeout=8) == 0


def steady_upload_layout():
    uploads = Queue()

    class Server(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def reply(self, status):
            self.send_response(status)
            self.send_header('Content-Length', '0')
            self.end_headers()

        def do_PUT(self):
            self.rfile.read(int(self.headers['Content-Length']))
            if self.path.startswith('/dav/source/'):
                proceed = Event()
                name = 'long-name.fixture' if '/nested-' in self.path else 'short-name.fixture'
                uploads.put((name, proceed))
                assert proceed.wait(10), 'Upload was not released by the test'
            self.reply(201)

        def do_MKCOL(self): self.reply(201)
        def do_MOVE(self): self.reply(201)
        def do_GET(self): self.reply(404)

    app = SessionUI('upload-layout')
    source = app.root/'source'
    nested = source.joinpath(*(['nested-' + 'a' * 90] * 5))
    nested.mkdir(parents=True)
    with HTTPServer(('127.0.0.1', 0), Server) as server:
        server_thread = Thread(target=server.serve_forever, daemon=True)
        server_thread.start()
        app.cfg.parent.mkdir(parents=True)
        app.cfg.write_text(f'folders = [{json.dumps(str(source))}]\n[[targets]]\n'
                           f'type = "webdav"\nurl = "http://127.0.0.1:{server.server_port}/dav"\n'
                           'user = "qa"\n[settings]\nauto_sync = false\nsync_mode = "backup"\n')
        try:
            with patch.dict(os.environ, CLOUDSAVE_WEBDAV_PASSWORD='fixture'):
                app.launch()
            heights = {}
            # Compare label heights without assuming which parallel upload the
            # UI highlights. Each run introduces exactly one new file.
            for path in (source/'short-name.fixture', nested/'long-name.fixture'):
                path.write_bytes(b'layout fixture')
                app.click('Jetzt synchronisieren')
                name, proceed = uploads.get(timeout=8)
                try:
                    assert name == path.name, (name, path.name)
                    eventually(lambda: any(name in label for label in app.labels()))
                    time.sleep(.1)
                    nodes = app.tree()
                    subtitle = next(n for n in nodes if n['type'] == 'GtkLabel' and n['mapped'] and name in n['label'])
                    status = nodes[subtitle['parent']]
                    banner = nodes[status['parent']]
                    heights[name] = (subtitle['height'], banner['height'])
                finally:
                    proceed.set()
                eventually(lambda: 'Alles synchronisiert' in app.labels())
                path.unlink()
            assert heights['short-name.fixture'] == heights['long-name.fixture'], heights
        finally:
            app.stop()
            server.shutdown()
            server_thread.join(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    assert os.environ.get('CLOUDSAVE_ISOLATED_TEST_BUS') == '1', 'Use a disposable dbus-run-session'
    ui.BINARY = args.binary.resolve()
    ui.OUT = args.output.resolve()
    ui.OUT.mkdir(parents=True, exist_ok=False)
    ui.run('cc', '-std=c11', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror', '-O2',
           '-o', str(ui.HERE/'gtk_qa_bridge.so'), str(ui.HERE/'gtk_qa_bridge.c'), '-ldl')
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    watcher = Watcher(connection)
    notifications = Notifications(connection)
    checks = []

    def item_property(name):
        return call(connection, ITEM, 'org.freedesktop.DBus.Properties', 'Get', '(ss)', (SNI, name))[0]

    def wait_status(label):
        eventually(lambda: item_property('ToolTip')[3].startswith(label))
        return bytes(item_property('IconPixmap')[0][2])

    def menu_rows():
        return call(connection, MENU, DBUSMENU, 'GetLayout', '(iias)', (0, -1, []))[1][2]

    def menu_click(label):
        row = next(row for row in menu_rows() if row[1]['label'] == label)
        assert row[1]['enabled']
        call(connection, MENU, DBUSMENU, 'Event', '(isvu)', (row[0], 'clicked', GLib.Variant('i', 0), 0))

    def passed(name):
        checks.append(name)
        print('PASS', name, flush=True)
        (ui.OUT/'results.json').write_text(json.dumps(checks, indent=2, ensure_ascii=False))

    app = SessionUI('tray-cycles', folders=[], run_in_background=True, auto_sync=True)
    source, target = app.root/'Dokumente', app.root/'Backup'
    source.mkdir(); target.mkdir()
    (source/'hello.txt').write_text('first')
    app.write_config([source], [target])
    try:
        app.launch()
        eventually(lambda: len(watcher.registrations) == 1)
        original_sender = watcher.registrations[0][0]
        assert watcher.registrations[0][1] == ITEM
        props = call(connection, ITEM, 'org.freedesktop.DBus.Properties', 'GetAll', '(s)', (SNI,))[0]
        assert props['Status'] == 'Active' and props['Id'] == APP and props['Menu'] == MENU
        assert props['ItemIsMenu'] is True and props['IconName'] == ''
        for width, height, pixels in props['IconPixmap']:
            assert width == height and width in (24, 32, 48, 64) and len(pixels) == width*height*4
            assert any(pixels[i] > 200 and pixels[i+1] > 200 and 60 < pixels[i+2] < 200 and pixels[i+3] < 100 for i in range(0,len(pixels),4)), 'Missing orange ARGB pixels'
            assert any(all(channel > 220 for channel in pixels[i:i+4]) for i in range(0,len(pixels),4)), 'Missing white cloud'
        assert len(props['IconPixmap']) == 4
        passed('Tray supplies the embedded orange cloud in four sizes and requests a menu on normal click')

        revision, layout = call(connection, MENU, DBUSMENU, 'GetLayout', '(iias)', (0, -1, []))
        assert revision >= 1
        labels = [child[1]['label'] for child in layout[2]]
        assert all(label in labels for label in ['Cloudsave öffnen','Beenden','Aktive Jobs','Letzte Jobs'])
        group = call(connection, MENU, DBUSMENU, 'GetGroupProperties', '(aias)', ([1, 2], ['label']))[0]
        assert len(group) == 2 and all(list(properties) == ['label'] for _, properties in group)
        assert call(connection, MENU, DBUSMENU, 'GetProperty', '(is)', (1, 'label'))[0] == 'Cloudsave öffnen'
        assert call(connection, MENU, DBUSMENU, 'AboutToShow', '(i)', (0,)) == (False,)
        requested = []
        subscription = connection.signal_subscribe(APP, DBUSMENU, 'ItemActivationRequested', MENU, None,
            Gio.DBusSignalFlags.NONE, lambda *args: requested.append(args[-1].unpack()))
        call(connection, ITEM, SNI, 'Activate', '(ii)', (0,0))
        eventually(lambda: bool(requested))
        assert requested[-1] == (0,0)
        connection.signal_unsubscribe(subscription)
        passed('Panel menu exposes job sections, explicit Open/Quit and requests the root menu on activation')

        for cycle in range(5):
            app.close_window()
            status = call(connection, ITEM, 'org.freedesktop.DBus.Properties', 'Get', '(ss)', (SNI, 'Status'))[0]
            assert status == 'Active'
            if cycle == 0:
                eventually(lambda: (target/'Dokumente/hello.txt').exists())
                database = sqlite3.connect(app.root/'state/cloudsave/state.db')
                database.execute('BEGIN EXCLUSIVE')
                (source/'hello.txt').write_text('updated while hidden')
                def menu_labels():
                    return [child[1]['label'] for child in call(connection, MENU, DBUSMENU, 'GetLayout', '(iias)', (0,-1,[]))[1][2]]
                try:
                    eventually(lambda: any('Synchronisierung' in label and 'Ordner werden geprüft' in label for label in menu_labels()), 10)
                    assert 'Keine aktiven Jobs' not in menu_labels()
                    busy_icon = wait_status('Datei-Auftrag läuft')
                    menu_click('Pausieren')
                    eventually(lambda: any('Pause nach der aktuellen Datei' in label for label in menu_labels()))
                    passed('Tray shows an actual active sync while the real state database is busy')
                finally:
                    database.rollback(); database.close()
                eventually(lambda: any('Auftrag pausiert' in label for label in menu_labels()))
                assert (target/'Dokumente/hello.txt').read_text() == 'first'
                menu_click('Fortsetzen')
                eventually(lambda: (target/'Dokumente/hello.txt').exists() and
                           (target/'Dokumente/hello.txt').read_text() == 'updated while hidden', 12)
                eventually(lambda: any('hochgeladen' in label for label in menu_labels()))
                assert wait_status('Synchronisierung erfolgreich') != busy_icon
                passed('Real automatic sync continues hidden and its result enters tray history')
            if cycle == 1:
                call(connection, MENU, DBUSMENU, 'Event', '(isvu)', (1, 'clicked', GLib.Variant('i', 0), 0))
            elif cycle == 2:
                app.secondary()
            else:
                if cycle == 3: call(connection, ITEM, SNI, 'SecondaryActivate', '(ii)', (0,0))
                else: call(connection, MENU, DBUSMENU, 'Event', '(isvu)', (1,'clicked',GLib.Variant('i',0),0))
            app.assert_window(True)
            app.secondary(background=True)
            app.assert_window(True)
        assert len(watcher.registrations) == 1
        passed('Five close/reopen cycles reuse one window and process; secondary background call keeps window visible')

        watcher.release()
        eventually(lambda: not connection.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus',
                   'org.freedesktop.DBus', 'NameHasOwner', GLib.Variant('(s)', (WATCHER,)), None,
                   Gio.DBusCallFlags.NONE, 5000, None).unpack()[0])
        # Let GIO observe the vanished watcher before its name reappears.
        for _ in range(5):
            GLib.MainContext.default().iteration(False)
            time.sleep(.02)
        watcher.own()
        eventually(lambda: len(watcher.registrations) == 2)
        assert watcher.registrations[-1] == (original_sender, ITEM)
        passed('Panel watcher restart re-registers the same tray item')
        app.close_window()
        call(connection, MENU, DBUSMENU, 'Event', '(isvu)', (2, 'clicked', GLib.Variant('i', 0), 0))
        assert app.proc.wait(timeout=8) == 0
        passed('Tray Quit exits even while window is hidden')
    finally:
        app.stop()

    watcher.release()
    app = SessionUI('background-start', folders=[], run_in_background=True)
    try:
        app.launch(background=True)
        passed('--background is accepted and creates one hidden window')
        watcher.own()
        eventually(lambda: len(watcher.registrations) == 3)
        app.secondary()
        app.assert_window(True)
        app.close_window()
        call(connection, MENU, DBUSMENU, 'Event', '(isvu)', (1,'clicked',GLib.Variant('i',0),0))
        app.assert_window(True)
        passed('Late panel startup and launcher activation work after background startup')
        app.click('Einstellungen')
        button = app.find('GtkButton', 'App beenden')
        (app.root/'control/command').write_text(f'999\tclick\t{button["id"]}\t\n')
        assert app.proc.wait(timeout=8) == 0
        passed('Settings Quit exits explicitly')
    finally:
        app.stop()

    app = SessionUI('background-off', folders=[], run_in_background=False)
    try:
        app.launch()
        app.expect_exit_on_close()
        passed('Disabling background mode makes the first close exit')
    finally:
        app.stop()
    app = SessionUI('tray-download', folders=[], auto_sync=False)
    backup = app.root/'Backup'; (backup/'Archive').mkdir(parents=True)
    saved = backup/'Archive/saved.txt'; saved.write_text('saved')
    destination = app.root/'restored'; destination.mkdir()
    app.write_config([], [backup])
    try:
        app.launch(); app.click('Wiederherstellen')
        eventually(lambda: any('1 Sicherungsdatei gefunden' in label for label in app.labels()))
        app.set_active('GtkCheckButton', True)
        app.click('Wiederherstellungsordner wählen …')
        ui.run('xdotool','key','ctrl+l'); time.sleep(.2)
        entry=app.find('GtkFileChooserEntry'); app.command('text',entry['id'],str(destination)); ui.run('xdotool','key','Return')
        if not ui.eventually(lambda:not any(n['type']=='GtkFileChooserDialog' and n['mapped'] for n in app.tree()),3): app.click('_Select')
        saved.unlink(); os.mkfifo(saved)
        app.click('Wiederherstellen')
        def download_labels():
            return [child[1]['label'] for child in call(connection,MENU,DBUSMENU,'GetLayout','(iias)',(0,-1,[]))[1][2]]
        eventually(lambda:any('Download: Archive/saved.txt' in label for label in download_labels()))
        with saved.open('wb', buffering=0) as writer:
            writer.write(b'sa')
            eventually(lambda: any('2 B / 5 B' in label for label in download_labels()))
            menu_click('Auftrag abbrechen')
            eventually(lambda: any('Abbruch angefordert' in label for label in download_labels()))
            writer.write(b'ved')
        eventually(lambda: any('Abgebrochen' in label for label in download_labels()))
        assert not (destination/'Archive/saved.txt').exists()
        assert not list(destination.rglob('.cloudsave-restore-*'))
        saved.unlink()
        saved.write_bytes(b'saved')
        app.click('Wiederherstellen')
        eventually(lambda:(destination/'Archive/saved.txt').exists())
        eventually(lambda:any('1 heruntergeladen' in label for label in download_labels()))
        assert (destination/'Archive/saved.txt').read_bytes() == b'saved'
        passed('Tray shows byte progress, cancels without publishing a partial restore, and reports a successful retry')
    finally:
        app.stop()

    app = SessionUI('tray-status', folders=[], auto_sync=False, run_in_background=True)
    source, working, broken = (app.root/name for name in ('source', 'backup', 'second-backup'))
    source.mkdir()
    (source/'save.txt').write_text('save data')
    broken.write_text('a file cannot hold a backup')
    app.write_config([source], [broken, working])
    icons_changed = []
    subscription = connection.signal_subscribe(APP, SNI, 'NewIcon', ITEM, None,
        Gio.DBusSignalFlags.NONE, lambda *_: icons_changed.append(True))
    try:
        app.launch()
        paused = wait_status('Automatische Synchronisierung pausiert')
        app.set_active('GtkSwitch', True)
        ready = wait_status('Bereit · noch kein erfolgreicher Sync')
        eventually(lambda: len(icons_changed) >= 1)

        app.click('Jetzt synchronisieren')
        failure = wait_status('Fehler · Sicherung prüfen')
        assert (working/'source/save.txt').read_text() == 'save data'
        eventually(lambda: len(notifications.shown) == 1)
        assert notifications.shown[0][3] == 'Synchronisierung mit Fehlern'
        app.close_window()
        connection.emit_signal(None, '/org/freedesktop/Notifications', NOTIFICATIONS,
                               'ActionInvoked', GLib.Variant('(us)', (1, 'default')))
        eventually(lambda: any(n['type'] == 'GtkApplicationWindow' and n['mapped'] for n in app.tree()))

        app.click('Jetzt synchronisieren')
        eventually(lambda: sum('1 Fehler' in row[1]['label'] for row in menu_rows()) == 2)
        assert len(notifications.shown) == 1
        passed('A failed sync sends a desktop notification without repeating it for the same outstanding failure')

        broken.unlink()
        broken.mkdir()
        app.click('Jetzt synchronisieren')
        success = wait_status('Synchronisierung erfolgreich')
        assert (broken/'source/save.txt').read_text() == 'save data'
        eventually(lambda: len(icons_changed) >= 3)
        assert len({ready, paused, busy_icon, failure, success}) == 5
        passed('Tray icon changes across five states and only clears a failed sync after a successful retry')

        app.set_active('GtkSwitch', False)
        wait_status('Automatische Synchronisierung pausiert')
        previous_jobs = [row[1]['label'] for row in menu_rows() if 'hochgeladen' in row[1]['label']]
        assert len(previous_jobs) == 3
        menu_click('Beenden')
        assert app.proc.wait(timeout=8) == 0
        app.stop()
        app.launch(background=True)
        wait_status('Automatische Synchronisierung pausiert')
        assert [row[1]['label'] for row in menu_rows() if 'hochgeladen' in row[1]['label']] == previous_jobs
        passed('Tray history retains the actual completed jobs across app restart')
    finally:
        connection.signal_unsubscribe(subscription)
        app.stop()
    steady_upload_layout()
    passed('Alternating short and long WebDAV upload paths leave the dashboard banner height fixed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
