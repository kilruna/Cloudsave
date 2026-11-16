#!/usr/bin/python3
"""Native GTK + real session D-Bus lifecycle regression (isolated test bus only).

Run under dbus-run-session and xvfb-run; the panel watcher is a protocol fixture,
while the app, window actions, secondary process, and background sync are real.
"""
import argparse
import json
import os
import pathlib
import subprocess
import sys
import time

from gi.repository import Gio, GLib
import run_ui_tests as ui

APP = ui.APP
ITEM = '/StatusNotifierItem'
MENU = '/MenuBar'
SNI = 'org.kde.StatusNotifierItem'
WATCHER = 'org.kde.StatusNotifierWatcher'
DBUSMENU = 'com.canonical.dbusmenu'


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
    checks = []

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
        passed('Tray registered and Active while window is visible')

        revision, layout = call(connection, MENU, DBUSMENU, 'GetLayout', '(iias)', (0, -1, []))
        assert revision == 1 and [child[1]['label'] for child in layout[2]] == ['Cloudsave öffnen', 'Beenden']
        group = call(connection, MENU, DBUSMENU, 'GetGroupProperties', '(aias)', ([1, 2], ['label']))[0]
        assert len(group) == 2 and all(list(properties) == ['label'] for _, properties in group)
        assert call(connection, MENU, DBUSMENU, 'GetProperty', '(is)', (1, 'label'))[0] == 'Cloudsave öffnen'
        assert call(connection, MENU, DBUSMENU, 'AboutToShow', '(i)', (0,)) == (False,)
        passed('Panel menu exposes opening and quitting through DBusMenu')

        for cycle in range(5):
            app.close_window()
            status = call(connection, ITEM, 'org.freedesktop.DBus.Properties', 'Get', '(ss)', (SNI, 'Status'))[0]
            assert status == 'Active'
            if cycle == 0:
                (source/'hello.txt').write_text('updated while hidden')
                eventually(lambda: (target/'Dokumente/hello.txt').exists() and
                           (target/'Dokumente/hello.txt').read_text() == 'updated while hidden', 12)
                passed('Real automatic sync continues with the window hidden')
            if cycle == 1:
                call(connection, MENU, DBUSMENU, 'Event', '(isvu)', (1, 'clicked', GLib.Variant('i', 0), 0))
            elif cycle == 2:
                app.secondary()
            else:
                call(connection, ITEM, SNI, 'Activate' if cycle != 3 else 'SecondaryActivate', '(ii)', (0, 0))
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
        call(connection, ITEM, SNI, 'Activate', '(ii)', (0, 0))
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
    return 0


if __name__ == '__main__':
    sys.exit(main())
