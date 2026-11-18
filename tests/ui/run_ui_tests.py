#!/usr/bin/python3
"""GTK regression suite from Cloudsave issue #10 (including its test-only bridge).
Run: /usr/bin/python3 run_ui_tests.py --output /path/to/new-results
Or: xvfb-run /usr/bin/python3 run_ui_tests.py --binary /path/to/cloudsave --output ...
Exit 0=all assertions pass, 1=product regressions, 2=harness/setup error.
The default tests the installed Flatpak. --binary explicitly tests a native build;
its caller must isolate networking. No real credentials or user data are used.
"""
import argparse, hashlib, json, os, pathlib, signal, subprocess, sys, tempfile, time, tomllib, traceback
os.environ['GDK_BACKEND'] = 'x11'
import gi
gi.require_version('Gdk', '3.0')
gi.require_version('GdkX11', '3.0')
from gi.repository import Gdk, GdkX11, GLib

HERE = pathlib.Path(__file__).resolve().parent
APP = 'org.cloudsave.Cloudsave'
RESULTS = []
OUT = None
BINARY = None

def run(*args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs).stdout.strip()

def eventually(fn, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if fn(): return True
        time.sleep(.1)
    return False

def record(case, title, ok, observed, expected, evidence=None):
    row = dict(id=case, title=title, status='PASS' if ok else 'FAIL', expected=expected,
               observed=observed, evidence=evidence or [])
    RESULTS.append(row)
    (OUT/'results.json').write_text(json.dumps(RESULTS, indent=2, ensure_ascii=False))
    print(f"{row['status']:4} {case} {title}: {observed}", flush=True)

class UI:
    def __init__(self, name, folders=None, targets=None, home_dirs=(), home_files=(), **settings):
        self.name = name
        scratch = os.environ.get('TMPDIR')
        if not scratch: raise RuntimeError('Set TMPDIR to a task-owned scratch directory, not system /tmp')
        self.root = pathlib.Path(tempfile.mkdtemp(prefix='cloudsave-ui-'+name+'-', dir=scratch))
        os.chmod(self.root, 0o700)
        for d in ('config','state','data','cache','home','control'):
            (self.root/d).mkdir()
        for directory in home_dirs:
            (self.root/'home'/directory).mkdir(parents=True, exist_ok=True)
        for filename in home_files:
            (self.root/'home'/filename).write_text('synthetic fixture')
        self.cfg = self.root/'config/cloudsave/config.toml'
        self.settings = dict(auto_sync=False, autostart=False, run_in_background=False,
                             sync_interval_minutes=1)
        self.settings.update(settings)
        if folders is not None:
            self.write_config(folders, targets or [])
        self.proc = None
        self.seq = 0
        self.nodes = []
        self.xid = None

    def write_config(self, folders, targets):
        self.cfg.parent.mkdir(parents=True, exist_ok=True)
        q = lambda value: json.dumps(str(value), ensure_ascii=False)
        text = 'folders = [' + ', '.join(q(p) for p in folders) + ']\n'
        if not targets: text += 'targets = []\n'
        for target in targets:
            text += '\n[[targets]]\ntype = "local"\npath = '+q(target)+'\n'
        text += '\n[settings]\n'
        for key, value in self.settings.items():
            text += key+' = '+(str(value).lower() if isinstance(value,bool) else str(value))+'\n'
        self.cfg.write_text(text)

    def launch(self):
        # Flatpak overwrites --env=XDG_* during initialization. Set these INSIDE
        # its sandbox using env, otherwise the user's real config gets loaded.
        root = str(self.root)
        args = ['flatpak','run','--no-documents-portal','--nosocket=session-bus',
                '--no-session-bus','--nosocket=wayland','--socket=x11','--unshare=network',
                '--nofilesystem=home',
                '--nofilesystem=xdg-config/autostart',f'--filesystem={root}',
                f'--filesystem={HERE}:ro','--command=env',APP,
                f'XDG_CONFIG_HOME={root}/config',f'XDG_STATE_HOME={root}/state',
                f'XDG_DATA_HOME={root}/data',f'XDG_CACHE_HOME={root}/cache',
                f'HOME={root}/home','GDK_BACKEND=x11',
                'DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent-cloudsave-ui-qa-bus',
                f'CLOUDSAVE_QA_DIR={root}/control', f'LD_PRELOAD={HERE}/gtk_qa_bridge.so',
                '/app/bin/cloudsave']
        if BINARY:
            args = ['env', f'XDG_CONFIG_HOME={root}/config', f'XDG_STATE_HOME={root}/state',
                    f'XDG_DATA_HOME={root}/data', f'XDG_CACHE_HOME={root}/cache',
                    f'HOME={root}/home', 'GDK_BACKEND=x11',
                    'DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent-cloudsave-ui-qa-bus',
                    f'CLOUDSAVE_QA_DIR={root}/control', f'LD_PRELOAD={HERE}/gtk_qa_bridge.so',
                    str(BINARY)]
        self.logfile = (OUT/(self.name+'.stderr.log')).open('w')
        self.proc = subprocess.Popen(args, stdout=self.logfile, stderr=self.logfile, start_new_session=True)
        (OUT/(self.name+'.launch.json')).write_text(json.dumps(args, indent=2))
        self.command('dump', timeout=12)
        # Do not infer isolation from argv: verify the real executable environment
        # before performing any UI actions.
        found = []
        for p in pathlib.Path('/proc').iterdir():
            if not p.name.isdigit(): continue
            try:
                if p.joinpath('comm').read_text().strip() != 'cloudsave': continue
                env = dict(part.split(b'=',1) for part in p.joinpath('environ').read_bytes().split(b'\0') if b'=' in part)
                if env.get(b'CLOUDSAVE_QA_DIR') == str(self.root/'control').encode():
                    assert env[b'XDG_CONFIG_HOME'] == str(self.root/'config').encode()
                    assert env[b'XDG_STATE_HOME'] == str(self.root/'state').encode()
                    assert env[b'HOME'] == str(self.root/'home').encode()
                    found.append(int(p.name))
            except (FileNotFoundError, PermissionError): pass
        if len(found)!=1: raise RuntimeError(f'Isolation could not be verified: {found}')
        self.pid=found[0]
        windows = run('xdotool','search','--class','^cloudsave$').splitlines()
        # Only one owned QA X11 window is launched at a time; the user's native
        # Wayland window is not part of X11 window enumeration. Refuse ambiguity.
        visible=[]
        for w in windows:
            info=run('xwininfo','-id',w)
            if 'IsViewable' in info and '"Cloudsave"' in info: visible.append(w)
        if len(visible)!=1: raise RuntimeError(f'Ambiguous QA X11 window: {visible}')
        self.xid=int(visible[0])
        self.snapshot('initial')
        return self

    def command(self, verb, idx=-1, value='', timeout=6):
        self.seq += 1
        path=self.root/'control'
        temp=path/'command.tmp'
        temp.write_text(f'{self.seq}\t{verb}\t{idx}\t{value}\n')
        temp.replace(path/'command')
        def ready():
            if self.proc and self.proc.poll() is not None:
                raise RuntimeError(f'QA process exited: {self.proc.returncode}')
            try: return (path/'response').read_text().split('\t')[0] == str(self.seq)
            except FileNotFoundError: return False
        if not eventually(ready, timeout): raise TimeoutError(f'GTK bridge did not answer {verb}')
        response=(path/'response').read_text().strip().split('\t')
        if response[1]!='1': raise RuntimeError(f'Widget command rejected: {verb} index={idx}')
        self.read_tree()
        return self.nodes

    def read_tree(self):
        self.nodes=[]
        for line in (self.root/'control/tree.tsv').read_text().splitlines():
            cols=line.split('\t')
            if len(cols)!=12: raise RuntimeError(f'Bad widget row: {line!r}')
            i,parent,typ,visible,mapped,sensitive,active,width,height,label,text,value=cols
            self.nodes.append(dict(id=int(i), parent=int(parent), type=typ, visible=int(visible),
                mapped=int(mapped), sensitive=int(sensitive), active=int(active), width=int(width),
                height=int(height),label=label,text=text,value=float(value)))

    def tree(self):
        self.command('dump')
        return self.nodes

    def descendants(self, node):
        ids={node['id']}
        result=[]
        for item in self.nodes:
            if item['parent'] in ids:
                ids.add(item['id']);result.append(item)
        return result

    def find(self, typ=None, label=None, occurrence=0, mapped=True):
        self.tree()
        def labelled(n):
            if label is None or n['label']==label or any(c['label']==label for c in self.descendants(n)):
                return True
            if n['type']=='GtkCheckButton':
                p=n['parent']
                while p>=0:
                    parent=self.nodes[p]
                    if parent['type']=='AdwActionRow':
                        return any(c['label']==label for c in self.descendants(parent))
                    p=parent['parent']
            return False
        matches=[n for n in self.nodes if (typ is None or n['type']==typ)
                 and (not mapped or n['mapped']) and labelled(n)]
        if len(matches)<=occurrence: raise LookupError(f'Widget not found: {typ} {label} matches={len(matches)}')
        return matches[occurrence]

    def click(self, label):
        n=self.find('GtkButton', label)
        self.command('click',n['id']); time.sleep(.4)

    def set_active(self, typ, value, label=None, occurrence=0):
        n=self.find(typ,label,occurrence)
        self.command('active',n['id'],int(value));time.sleep(.2)

    def labels(self):
        return [n['label'] for n in self.tree() if n['type']=='GtkLabel' and n['mapped']]

    def idle(self):
        return eventually(lambda: bool({'Endpunkt einrichten', 'Keine Ordner ausgewählt'}.intersection(self.labels()))
                          or self.find('GtkButton','Jetzt synchronisieren',mapped=False)['sensitive'],10)

    def expand(self, label):
        node = self.find('GtkExpander', label)
        self.command('activate', node['id'])
        time.sleep(.25)

    def snapshot(self, tag):
        self.tree()
        stem=f'{self.name}-{tag}'
        (OUT/(stem+'.tree.tsv')).write_text((self.root/'control/tree.tsv').read_text())
        evidence=[stem+'.tree.tsv']
        if self.xid:
            display=Gdk.Display.get_default()
            window=GdkX11.X11Window.foreign_new_for_display(display,self.xid)
            window.set_events(window.get_events() | Gdk.EventMask.STRUCTURE_MASK)
            display.sync()
            context=GLib.MainContext.default()
            while context.pending(): context.iteration(False)
            # Foreign GdkWindow sizes can stay cached after xdotool resizes.
            info=run('xwininfo','-id',str(self.xid))
            dimensions={line.split(':',1)[0].strip():int(line.split(':',1)[1]) for line in info.splitlines()
                        if line.strip().startswith(('Width:','Height:'))}
            if window.is_viewable():
                pixbuf=Gdk.pixbuf_get_from_window(window,0,0,dimensions['Width'],dimensions['Height'])
                if pixbuf:
                    pixbuf.savev(str(OUT/(stem+'.png')),'png',[],[])
                    evidence.insert(0,stem+'.png')
        return evidence

    def stop(self):
        if self.proc and self.proc.poll() is None:
            os.killpg(self.proc.pid,signal.SIGTERM)
            try:self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid,signal.SIGKILL);self.proc.wait(timeout=5)
        if hasattr(self,'logfile'):self.logfile.close()
        # Consume DestroyNotify before another process can reuse the same XID.
        # Otherwise GDK may associate its stale foreign window with the new app.
        Gdk.Display.get_default().sync()
        context=GLib.MainContext.default()
        while context.pending(): context.iteration(False)
        self.xid=None
        if self.cfg.exists():
            (OUT/(self.name+'.config.toml')).write_text(self.cfg.read_text())

    def __enter__(self):
        try:return self.launch()
        except Exception:
            self.stop()
            raise
    def __exit__(self,*args):self.stop()


def onboarding():
    with UI('onboarding', home_dirs=('Documents', 'Pictures', 'Projekte/nested', '.hidden'), home_files=('notes.txt',)) as ui:
        record('UI-01','Welcome screen', 'Einrichtung starten' in ui.labels(),
               'Welcome and setup button rendered', 'Welcome screen offers setup',ui.snapshot('welcome'))
        close_buttons=[n for n in ui.tree() if n['type']=='GtkButton' and n['mapped'] and any(c['type']=='GtkImage' for c in ui.descendants(n))]
        # Window controls have native accessible labels, but the widget tree can
        # identify the actual GtkWindowControls containers without locale assumptions.
        controls=[n for n in ui.nodes if n['type']=='GtkWindowControls' and n['mapped'] and any(c['type']=='GtkButton' for c in ui.descendants(n))]
        record('UI-02','One title bar / window-control set', len(controls)<=1,
               f'{len(controls)} mapped nonempty GtkWindowControls containers',
               'At most one mapped nonempty control set',ui.snapshot('duplicate-controls'))
        ui.click('Einrichtung starten')
        checks=[n for n in ui.tree() if n['type']=='GtkCheckButton' and n['mapped']]
        labels=ui.labels()
        offered=[label for label in labels if label in ('Documents','Pictures','Projekte')]
        record('UI-03','Discover existing home folders without dummy suggestions',len(checks)==3 and
               offered==['Documents','Pictures','Projekte'] and not any(n['active'] for n in checks) and
               not any(label in labels for label in ('Dokumente','Bilder','Musik','Videos','Schreibtisch','.hidden','notes.txt','nested')),
               f'Offered folders={offered}; selected={sum(n["active"]==1 for n in checks)}',
               'Only real visible direct subfolders are offered alphabetically, initially unselected',ui.snapshot('home-folders'))
        record('UI-04','Empty selection blocks Next',not ui.find('GtkButton','Weiter')['sensitive'],
               f'Next sensitive={ui.find("GtkButton","Weiter")["sensitive"]}','Next disabled without sources')
        ui.set_active('GtkCheckButton',True)
        ui.click('Weiter')
        before=ui.snapshot('local-before-activate')
        local=ui.find('GtkCheckButton', 'Ordner oder Laufwerk')
        assert local['active']==1
        assert sum(node['type']=='GtkCheckButton' and node['mapped'] for node in ui.tree()) == 2
        assert not any(node['type']=='AdwEntryRow' and node['mapped'] for node in ui.tree())
        ui.command('activate',local['id']);time.sleep(.4)
        next_enabled=ui.find('GtkButton','Weiter')['sensitive']
        roots=[n for n in ui.tree() if n['parent']==-1 and n['visible']]
        browse=[n for n in ui.nodes if n['type']=='GtkButton' and n['mapped'] and n['sensitive'] and n['label']=='Ordner auswählen …']
        # An explicit, reusable browse button satisfies this scenario without
        # depending on the initially-active radio firing a toggled signal.
        record('UI-05','Initially selected local target is actionable',bool(browse) or len(roots)>1 or bool(next_enabled),
               f'{len(roots)} visible top-level window(s), Next sensitive={next_enabled}, initial local radio active',
               'Activating local target offers directory selection or an explicit browse control', before+ui.snapshot('local-stuck'))
        ui.set_active('GtkCheckButton',True,'Cloud-Speicher')
        record('UI-06','Empty WebDAV credentials block Next',not ui.find('GtkButton','Weiter')['sensitive'],
               f'Next sensitive={ui.find("GtkButton","Weiter")["sensitive"]}','Next disabled without required fields')
        for title,value in [('Benutzername','qa-user'),('Server-URL','not a url'),('Passwort','synthetic-qa-value')]:
            typ='AdwPasswordEntryRow' if title=='Passwort' else 'AdwEntryRow'
            n=ui.find(typ,title);ui.command('text',n['id'],value)
        validations=[]
        for url,valid in [('',False),('  ',False),('not a url',False),('http://example.com',False),('ftp://example.com',False),('https://example.com/dav',True),('example.com:8443/dav',True)]:
            n=ui.find('AdwEntryRow','Server-URL');ui.command('text',n['id'],url)
            validations.append(bool(ui.find('GtkButton','Weiter')['sensitive'])==valid)
        n=ui.find('AdwEntryRow','Server-URL');ui.command('text',n['id'],'not a url')
        record('UI-07','Reject malformed WebDAV URL before continuing',all(validations),
               f'Next sensitive={ui.find("GtkButton","Weiter")["sensitive"]} for Server-URL="not a url"',
               'Malformed or non-HTTPS URLs cannot be submitted',ui.snapshot('invalid-url'))
        back=ui.find('AdwBackButton')
        button=next(n for n in ui.descendants(back) if n['type']=='GtkButton')
        ui.command('click',button['id']);time.sleep(.4)
        record('UI-08','Back navigation restores source page','Welche Ordner sollen gesichert werden?' in ui.labels(),
               'Source selection page restored','Back returns to previous wizard step')


def fixture(ui):
    source=ui.root/'source/Documents'
    source.mkdir(parents=True)
    (source/'save.txt').write_text('qa-save-v1\n')
    (source/'nested').mkdir()
    (source/'nested/ümlaut.txt').write_text('QA Unicode payload ✓\n')
    targets=[ui.root/'backup-one',ui.root/'backup-two']
    ui.write_config([source],targets)
    return source,targets


def dashboard():
    ui=UI('dashboard');source,targets=fixture(ui)
    try:
        ui.launch();ui.idle()
        ui.expand('Sicherungsdetails')
        empty_stats=all(text in ui.labels() for text in ('0','0 B','Noch nie'))
        files=[p/'Documents/save.txt' for p in targets]
        record('UI-09','Auto-sync OFF suppresses startup sync',not any(p.exists() for p in files),
               f'Target files created at startup: {[p.exists() for p in files]}; auto_sync=false',
               'No automatic target writes until explicit manual sync',ui.snapshot('auto-off-startup'))
        # Seed the backup explicitly so this test remains independent when the
        # auto-sync-OFF startup bug is fixed. Never depend on a preceding FAIL.
        ui.click('Jetzt synchronisieren');ui.idle()
        record('UI-10','Local multi-target sync including Unicode',all(
            (t/'Documents/save.txt').read_bytes()==(source/'save.txt').read_bytes() and
            (t/'Documents/nested/ümlaut.txt').read_bytes()==(source/'nested/ümlaut.txt').read_bytes() for t in targets),
            'Compared both target file payloads byte-for-byte','Both local targets contain identical source data')
        (source/'save.txt').write_text('qa-save-version-two-longer\n')
        ui.click('Jetzt synchronisieren');ui.idle()
        record('UI-11','Manual sync propagates changed content',all(p.read_bytes()==(source/'save.txt').read_bytes() for p in files),
               'Both targets match edited source','Explicit sync updates every target')
        expected_size=2*sum(p.stat().st_size for p in source.rglob('*') if p.is_file())
        live_stats='4' in ui.labels() and f'{expected_size} B' in ui.labels()
        narrow=True
        if BINARY:
            ui.expand('Sicherungsdetails')
            run('xdotool','windowsize',str(ui.xid),'560','650');time.sleep(.4)
            assert ui.find('GtkButton','Wiederherstellen')['sensitive']
            assert ui.find('GtkButton','Jetzt synchronisieren')['sensitive']
            ui.snapshot('overview-560')
            run('xdotool','windowsize',str(ui.xid),'560','900');time.sleep(.4)
            info=run('xwininfo','-id',str(ui.xid))
            width=int(next(line.split(':',1)[1] for line in info.splitlines() if line.strip().startswith('Width:')))
            narrow=width<=560 and all(text in ui.labels() for text in ('Gesicherte Ordner','Ziel 1','Ziel 2','Sicherungsdetails'))
            narrow=narrow and any(name.endswith('.png') for name in ui.snapshot('orange-narrow'))
            run('xdotool','windowsize',str(ui.xid),'1180','820');time.sleep(.4)
        record('UI-32','Dashboard shows actual backup statistics and adapts to narrow windows',empty_stats and live_stats and narrow,
               f'Empty state={empty_stats}; expected 4 copies / {expected_size} B={live_stats}; narrow layout={narrow}',
               'Zero before first sync; actual counts and bytes after sync; source and target panels remain reachable',ui.snapshot('orange-dashboard'))
        ui.click('Einstellungen')
        spin=ui.find('GtkSpinButton');ui.command('value',spin['id'],2)
        record('UI-12','Interval setting persists',tomllib.loads(ui.cfg.read_text())['settings']['sync_interval_minutes']==2,
               f'Persisted interval={tomllib.loads(ui.cfg.read_text())["settings"]["sync_interval_minutes"]}','Setting written to isolated config')
        for t in targets:(t/'.cloudsave-test').write_text('PREEXISTING USER DATA - KEEP ME')
        ui.click('Verbindung testen')
        eventually(lambda:any('✓' in s for s in ui.labels()))
        preserved=all((t/'.cloudsave-test').exists() and (t/'.cloudsave-test').read_text()=='PREEXISTING USER DATA - KEEP ME' for t in targets)
        record('UI-13','Connection test preserves existing target files',preserved,
               f'Pre-existing .cloudsave-test files remain: {[(t/".cloudsave-test").exists() for t in targets]}',
               'Connection test uses unique temporary files and never replaces/deletes user content',ui.snapshot('connection-test-deletes-sentinel'))
        before=[p.exists() for p in files]
        ui.set_active('GtkCheckButton',False)
        saved=tomllib.loads(ui.cfg.read_text())['folders']
        ui.stop();ui.launch();ui.idle()
        assert 'Keine Ordner ausgewählt' in ui.labels()
        assert 'Alles synchronisiert' not in ui.labels()
        assert ui.find('GtkButton', 'Ordner hinzufügen')['sensitive']
        ui.click('Wiederherstellen')
        assert eventually(lambda: {'save.txt', 'ümlaut.txt'}.issubset(ui.labels()))
        after=[p.exists() for p in files]
        record('UI-14','Disabling source preserves existing backups',all(after),
               f'Before={before}; after={after}; persisted folders={saved}; no confirmation dialog',
               'Stopping backup of a folder does not silently purge its existing backups',ui.snapshot('deselect-deletes-backups'))
    finally:ui.stop()


def missing_source():
    ui=UI('missing-source');source,targets=fixture(ui)
    try:
        ui.launch();ui.idle()
        ui.click('Jetzt synchronisieren');ui.idle()
        files=[p/'Documents/save.txt' for p in targets]
        before=[p.exists() for p in files]
        ui.stop();ui.launch();ui.idle()
        source.rename(source.with_name('Documents-temporarily-unavailable'))
        ui.click('Jetzt synchronisieren');ui.idle()
        labels=ui.labels()
        record('UI-15','Unavailable source does not purge backup',all(p.exists() for p in files),
               f'Backup present before={before}; after={[p.exists() for p in files]}; UI labels={labels[:8]}',
               'Missing/unmounted source produces safe error, leaving every existing backup intact',ui.snapshot('source-missing'))
        ui.expand('Fehlerdetails')
        labels=ui.labels()
        record('UI-16','Sync error includes actionable diagnostic',any('Ordner nicht gefunden' in s for s in labels),
               ' | '.join(s for s in labels if 'Sync' in s or 'gesichert' in s),
               'Show failing source/target and error reason, not just error count',ui.snapshot('hidden-error-details'))
    finally:ui.stop()


def empty_targets():
    ui=UI('empty-targets')
    source=ui.root/'source';source.mkdir();(source/'save.txt').write_text('must not be reported as backed up')
    ui.write_config([source],[])
    try:
        ui.launch();ui.idle();labels=ui.labels()
        record('UI-17','No-target state is not reported as synchronized','Alles synchronisiert' not in labels,
               ' | '.join(s for s in labels if 'synchronisiert' in s or 'Endpunkt' in s),
               'No configured target: warn and disable or reject sync',ui.snapshot('false-success-no-target'))
        ui.click('Einstellungen')
        try:
            ui.click('App beenden')
        except RuntimeError:
            if ui.proc.poll() is None: raise
        record('UI-18','Quit button terminates app',eventually(lambda:ui.proc.poll() is not None),
               f'Process return code={ui.proc.poll()}','App beenden exits the owned test process')
    finally:ui.stop()

def sync_status():
    app = UI('sync-status')
    source, targets = fixture(app)
    blocked = targets[0].with_name('first-backup-away')
    try:
        app.launch(); app.click('Jetzt synchronisieren'); assert app.idle()
        copy = targets[0]/'Documents/save.txt'
        copy.unlink()
        app.click('Jetzt synchronisieren'); assert app.idle()
        assert copy.read_bytes() == (source/'save.txt').read_bytes()
        assert 'Alles synchronisiert' in app.labels()
        assert any('Letzte erfolgreiche Sicherung' in label for label in app.labels())

        targets[0].rename(blocked)
        targets[0].write_text('blocks the first backup target')
        (source/'save.txt').write_text('updated content sent to the other target')
        app.click('Jetzt synchronisieren'); assert app.idle()
        headings = ('Ziel 1', 'Ziel 2')
        reports = []
        for title in headings:
            heading = app.find('GtkLabel', title)
            card = app.nodes[app.nodes[heading['parent']]['parent']]
            reports.append({node['label'] for node in app.descendants(card) if node['mapped']})
        assert 'Ein Ziel braucht Aufmerksamkeit' in app.labels()
        assert 'Sicherungsordner nicht verfügbar' in reports[0] and 'Sicherung geprüft' in reports[1]
        assert 'Alles synchronisiert' not in app.labels()
        assert (targets[1]/'Documents/save.txt').read_bytes() == (source/'save.txt').read_bytes()
        assert (blocked/'Documents/save.txt').read_text() == 'qa-save-v1\n'
        app.expand('Fehlerdetails')
        assert any(str(targets[0]) in label and 'File exists' in label for label in app.labels())
        record('UI-42','Repair missing copies and report partial sync results',True,
               'Missing copy repaired; the blocked and successful target show their own outcome',
               'Partial failure preserves original data and exposes technical details',app.snapshot('partial-failure'))
    finally:
        app.stop()


def window_controls(ui):
    return [n for n in ui.tree() if n['type']=='GtkWindowControls' and n['mapped']
            and any(c['type']=='GtkButton' for c in ui.descendants(n))]


def choose_native_folder(ui, path):
    """Exercise GTK's real fallback chooser; this does not test portal grants."""
    ui.click('Ordner auswählen …')
    run('xdotool','key','ctrl+l');time.sleep(.2)
    entry=ui.find('GtkFileChooserEntry')
    ui.command('text',entry['id'],str(path))
    run('xdotool','key','Return')
    if not eventually(lambda:not any(n['type']=='GtkFileChooserDialog' and n['mapped'] for n in ui.tree()),3):
        ui.click('_Select')
    eventually(lambda:ui.find('GtkButton','Ordner auswählen …')['sensitive'])


def native_picker():
    # Native-only follow-up coverage: real chooser success/cancel/reselection,
    # explicit initial sync, and reuse in the add-endpoint flow. No portal claim.
    with UI('native-picker') as ui:
        source=ui.root/'home/Dokumente';source.mkdir()
        (ui.root/'home/Bilder').mkdir()
        (source/'save.txt').write_text('explicit first sync')
        first=ui.root/'backup-first';first.mkdir()
        second=ui.root/'backup-second';second.mkdir()
        extra=ui.root/'backup-extra';extra.mkdir()
        controls=[len(window_controls(ui))]
        ui.click('Einrichtung starten');controls.append(len(window_controls(ui)))
        ui.set_active('GtkCheckButton',True,'Dokumente')
        ui.click('Weiter');controls.append(len(window_controls(ui)))
        ui.click('Ordner auswählen …');ui.click('_Cancel')
        states=[not ui.find('GtkButton','Weiter')['sensitive'],bool(ui.find('GtkButton','Ordner auswählen …')['sensitive'])]
        choose_native_folder(ui,first)
        states.extend([str(first) in ui.labels(),bool(ui.find('GtkButton','Weiter')['sensitive'])])
        ui.click('Ordner auswählen …');ui.click('_Cancel')
        states.extend([str(first) in ui.labels(),bool(ui.find('GtkButton','Weiter')['sensitive'])])
        choose_native_folder(ui,second)
        ui.set_active('GtkCheckButton',True,'Cloud-Speicher')
        states.append(not ui.find('GtkButton','Weiter')['sensitive'])
        ui.set_active('GtkCheckButton',True,'Ordner oder Laufwerk')
        states.extend([str(second) in ui.labels(),bool(ui.find('GtkButton','Weiter')['sensitive'])])
        ui.click('Weiter');controls.append(len(window_controls(ui)))
        record('UI-19','Native chooser selection/cancel/reselection and wizard controls',all(states) and all(n==1 for n in controls),
               f'Chooser states={states}; window-control sets on wizard pages={controls}',
               'Cancel keeps valid selection; reselect and radio roundtrip work; one control set per page',ui.snapshot('done'))
        ui.set_active('GtkSwitch',False)
        ui.click('Sync einrichten & starten');ui.idle()
        record('UI-20','Explicit setup action starts first sync',
               (second/'Dokumente/save.txt').exists() and (second/'Dokumente/save.txt').read_bytes()==(source/'save.txt').read_bytes(),
               f'First sync wrote selected target={second}; old target empty={not any(first.iterdir())}',
               'Setup start action performs its requested sync',ui.snapshot('first-sync'))
        ui.click('Weiteren Endpunkt hinzufügen')
        ui.set_active('GtkCheckButton',True,'Cloud-Speicher')
        for title,value in [('Benutzername','qa-user'),('Server-URL','not a url'),('Passwort','synthetic-only')]:
            n=ui.find('AdwPasswordEntryRow' if title=='Passwort' else 'AdwEntryRow',title)
            ui.command('text',n['id'],value)
        invalid_blocked=not ui.find('GtkButton','Hinzufügen')['sensitive']
        choose_native_folder(ui,extra)
        ui.click('Hinzufügen')
        config=tomllib.loads(ui.cfg.read_text())
        ui.click('Jetzt synchronisieren');ui.idle()
        record('UI-21','Add endpoint reuses validation and native chooser',invalid_blocked and
               [t['path'] for t in config['targets']]==[str(second),str(extra)] and
               (extra/'Dokumente/save.txt').read_bytes()==(source/'save.txt').read_bytes() and len(window_controls(ui))==1,
               f'Malformed URL blocked={invalid_blocked}; targets={config["targets"]}',
               'Invalid endpoint blocked; explicit local selection adds a working second target',ui.snapshot('added-target'))


def empty_or_missing_home():
    for missing in (False, True):
        with UI('missing-home' if missing else 'empty-home') as ui:
            if missing:
                (ui.root/'home').rename(ui.root/'home-unavailable')
            ui.click('Einrichtung starten')
            labels=ui.labels()
            reason='Home-Verzeichnis konnte nicht gelesen werden' if missing else 'Keine sichtbaren Ordner'
            record('UI-23' if missing else 'UI-22','Missing home error' if missing else 'Empty home has no dummy folders',
                   not any(n['type']=='GtkCheckButton' and n['mapped'] for n in ui.tree()) and
                   not ui.find('GtkButton','Weiter')['sensitive'] and any(reason in label for label in labels) and
                   'Anderer Ordner hinzufügen …' in labels,
                   ' | '.join(label for label in labels if reason in label),
                   'Explain empty/unavailable home, keep manual folder picker, and require a selected source',ui.snapshot('home-state'))


def unavailable_keyring():
    with UI('unavailable-keyring', home_dirs=('Documents',)) as ui:
        ui.click('Einrichtung starten');ui.set_active('GtkCheckButton',True);ui.click('Weiter')
        ui.set_active('GtkCheckButton',True,'Cloud-Speicher')
        for title,value in [('Benutzername','synthetic-qa'),('Server-URL','https://example.invalid'),('Passwort','not-a-real-password')]:
            entry=ui.find('AdwPasswordEntryRow' if title=='Passwort' else 'AdwEntryRow',title)
            ui.command('text',entry['id'],value)
        ui.click('Weiter');ui.set_active('GtkSwitch',False);ui.click('Sync einrichten & starten')
        shown=eventually(lambda:any('System-Schlüsselbund' in text for text in ui.labels()))
        record('UI-24','Keyring failure keeps setup open and does not persist plaintext',shown and
               not ui.cfg.exists() and not list((ui.root/'config').rglob('*.secret')) and
               bool(ui.find('GtkButton','Sync einrichten & starten')['sensitive']),
               'Keyring unavailable on isolated bus; setup can retry',
               'Visible error, responsive GTK, no configured target or plaintext password',ui.snapshot('keyring-error'))


def backup_mode():
    ui=UI('backup-mode');source,targets=fixture(ui)
    try:
        ui.launch();ui.idle();ui.click('Jetzt synchronisieren');ui.idle()
        ui.click('Einstellungen');mode=ui.find('GtkDropDown');ui.command('selected',mode['id'],0)
        saved=tomllib.loads(ui.cfg.read_text())['settings'].get('sync_mode')=='backup'
        (source/'save.txt').unlink()
        ui.stop();ui.launch();ui.idle();ui.click('Jetzt synchronisieren');ui.idle()
        retained=all((target/'Documents/save.txt').exists() for target in targets)
        ui.click('Einstellungen');mode=ui.find('GtkDropDown');ui.command('selected',mode['id'],1)
        ui.click('Jetzt synchronisieren');ui.idle()
        record('UI-25','Backup mode persists across restart and mirror is explicit',saved and retained and
               all(not (target/'Documents/save.txt').exists() for target in targets),
               f'Backup setting persisted={saved}; deleted files retained={retained}',
               'Backup retains removed files; choosing mirror propagates their deletion')
    finally:ui.stop()


def open_exclusions(ui):
    menu=ui.find('GtkMenuButton')
    button=next(n for n in ui.descendants(menu) if n['type']=='GtkToggleButton')
    ui.command('click',button['id']);time.sleep(.25)


def exclusion_settings():
    ui=UI('exclusion-settings',exclude_patterns=['*.legacy']);source,targets=fixture(ui)
    (source/'old.tmp').write_text('keep existing backup')
    def patterns():return tomllib.loads(ui.cfg.read_text())['settings'].get('exclude_patterns',[])
    try:
        ui.launch();ui.idle();ui.click('Jetzt synchronisieren');ui.idle();ui.click('Einstellungen')
        compact=not any(n['type']=='GtkTextView' and n['mapped'] for n in ui.tree())
        open_exclusions(ui)
        ui.set_active('GtkCheckButton',True,'Temporäre Dateien')
        preset=set(patterns())=={'*.legacy','*.tmp','*.temp'}
        ui.set_active('GtkCheckButton',True,'Cache-Ordner')
        cache=set(patterns())=={'*.legacy','*.tmp','*.temp','.cache','cache','Cache'}
        ui.set_active('GtkCheckButton',False,'Cache-Ordner')
        deselect=set(patterns())=={'*.legacy','*.tmp','*.temp'}
        ui.click('Eigene Regel hinzufügen …')
        entry=ui.find('GtkEntry');ui.command('text',entry['id'],'[')
        invalid_rejected=not ui.find('GtkButton','Regel hinzufügen')['sensitive'] and set(patterns())=={'*.legacy','*.tmp','*.temp'}
        entry=ui.find('GtkEntry');ui.command('text',entry['id'],'*.bak');ui.click('Regel hinzufügen')
        custom=set(patterns())=={'*.legacy','*.tmp','*.temp','*.bak'}
        open_exclusions(ui);ui.click('Eigene Regel hinzufügen …');ui.click('Abbrechen')
        cancelled=set(patterns())=={'*.legacy','*.tmp','*.temp','*.bak'}
        open_exclusions(ui)
        label=ui.find('GtkLabel','*.legacy');row=ui.nodes[label['parent']]
        remove=next(n for n in ui.descendants(row) if n['type']=='GtkButton' and n['label']=='Entfernen')
        ui.command('click',remove['id']);time.sleep(.2)
        removed=set(patterns())=={'*.tmp','*.temp','*.bak'}
        run('xdotool','key','Escape');time.sleep(.2)
        (source/'old.tmp').unlink();(source/'new.tmp').write_text('ignore new file');(source/'new.bak').write_text('ignore custom rule')
        ui.click('Jetzt synchronisieren');ui.idle()
        safe=all((target/'Documents/old.tmp').exists() and not (target/'Documents/new.tmp').exists() and not (target/'Documents/new.bak').exists() for target in targets)
        ui.stop();ui.launch();ui.idle();ui.click('Einstellungen');open_exclusions(ui)
        persisted=ui.find('GtkCheckButton','Temporäre Dateien')['active']==1 and '*.bak' in ui.labels() and set(patterns())=={'*.tmp','*.temp','*.bak'}
        record('UI-26','Clickable exclusions, custom rule validation and backup preservation',compact and preset and cache and deselect and invalid_rejected and custom and cancelled and removed and safe and persisted,
               f'Compact={compact}; presets={preset and cache and deselect}; invalid blocked={invalid_rejected}; custom/cancel/remove={custom and cancelled and removed}; backup safety={safe}; persisted={persisted}',
               'No typing needed for presets; custom dialog validates before saving; cancellation keeps rules; existing excluded backups stay',ui.snapshot('exclusion-presets'))
    finally:ui.stop()


def automatic_changes():
    ui=UI('automatic-changes',auto_sync=True,exclude_patterns=['*.tmp']);source,targets=fixture(ui)
    try:
        ui.launch();ui.idle()
        ready=eventually(lambda:any('Live-Sync aktiv' in label for label in ui.labels()))
        original=(source/'save.txt').stat()
        (source/'save.txt').write_text('QA-SAVE-v2\n')
        os.utime(source/'save.txt',ns=(original.st_atime_ns,original.st_mtime_ns+1))
        propagated=eventually(lambda:all((target/'Documents/save.txt').exists() and (target/'Documents/save.txt').read_bytes()==(source/'save.txt').read_bytes() for target in targets),12)
        ui.idle();ui.set_active('GtkSwitch',False)
        previous=(targets[0]/'Documents/save.txt').read_bytes()
        (source/'save.txt').write_text('manual-only after auto switch is disabled, even longer')
        time.sleep(4.5)
        paused=all((target/'Documents/save.txt').read_bytes()==previous for target in targets) and any('Live-Sync pausiert' in label for label in ui.labels())
        ui.click('Jetzt synchronisieren');ui.idle()
        record('UI-27','Filesystem events synchronize after debounce and respect auto OFF',ready and propagated and paused and
               all((target/'Documents/save.txt').read_bytes()==(source/'save.txt').read_bytes() for target in targets),
               f'Watcher ready={ready}; propagated without manual sync={propagated}; auto OFF respected={paused}',
               'Automatic writes follow real changes; auto OFF pauses them; manual sync still works')
    finally:ui.stop()


def restore_ui():
    ui=UI('restore-ui');source,targets=fixture(ui)
    try:
        ui.launch();ui.idle();ui.click('Jetzt synchronisieren');ui.idle();ui.click('Sicherungen wiederherstellen')
        listed=eventually(lambda:any('Sicherungsdateien gefunden' in text for text in ui.labels()))
        assert {'Documents', 'Documents/nested', 'save.txt', 'ümlaut.txt'}.issubset(ui.labels())
        run('xdotool', 'windowsize', str(ui.xid), '560', '650')
        time.sleep(.4)
        search=ui.find('GtkSearchEntry')
        ui.command('text',search['id'],'NESTED/ÜMLAUT')
        assert eventually(lambda: 'ümlaut.txt' in ui.labels() and 'save.txt' not in ui.labels())
        ui.set_active('GtkCheckButton',True,'ümlaut.txt')
        search=ui.find('GtkSearchEntry')
        ui.command('text',search['id'],'save')
        assert eventually(lambda: 'save.txt' in ui.labels() and 'ümlaut.txt' not in ui.labels())
        ui.set_active('GtkCheckButton',True,'save.txt')
        assert any('2 ausgewählt' in label for label in ui.labels())
        output=ui.root/'restore-output';output.mkdir()
        ui.click('Wiederherstellungsordner wählen …');run('xdotool','key','ctrl+l');time.sleep(.2)
        entry=ui.find('GtkFileChooserEntry');ui.command('text',entry['id'],str(output));run('xdotool','key','Return')
        eventually(lambda:not any(node['type']=='GtkFileChooserDialog' and node['mapped'] for node in ui.tree()))
        ui.click('Wiederherstellen')
        restored=eventually(lambda:(output/'Documents/nested/ümlaut.txt').exists())
        exact=restored and (output/'Documents/save.txt').read_bytes()==(source/'save.txt').read_bytes() and (output/'Documents/nested/ümlaut.txt').read_bytes()==(source/'nested/ümlaut.txt').read_bytes()
        eventually(lambda:ui.find('GtkButton','Wiederherstellen')['sensitive'])
        (output/'Documents/save.txt').write_text('existing local edits must stay')
        ui.click('Wiederherstellen')
        refused=eventually(lambda:any('nicht überschrieben' in text for text in ui.labels()))
        record('UI-28','Browse and restore actual backup files without overwriting local changes',listed and exact and refused and (output/'Documents/save.txt').read_text()=='existing local edits must stay',
               f'Listed={listed}; byte-exact Unicode restore={exact}; overwrite refused={refused}',
               'Real backup listing, native destination chooser, exact file contents and overwrite protection',ui.snapshot('restored'))
    finally:ui.stop()



def endpoint_cards():
    ui=UI('endpoint-cards');source,targets=fixture(ui)
    (targets[0]/'Archive').mkdir(parents=True)
    (targets[0]/'Archive/old.txt').write_text('existing backup')
    targets[1].write_text('this file deliberately blocks the second endpoint')
    def card_nodes(title):
        heading=ui.find('GtkLabel',title)
        header=ui.nodes[heading['parent']]
        card=ui.nodes[header['parent']]
        return ui.descendants(card)
    def click_in_card(title,label):
        node=next(n for n in card_nodes(title) if n['type']=='GtkButton' and n['label']==label)
        ui.command('click',node['id']);time.sleep(.4)
    try:
        ui.launch();ui.idle()
        cards=[card_nodes('Ziel 1'),card_nodes('Ziel 2')]
        add=ui.find('GtkButton','Weiteren Endpunkt hinzufügen')
        separate=all(add['id'] not in [n['id'] for n in nodes] and
                     sum(n['type']=='GtkButton' and n['label']=='Verbindung testen' for n in nodes)==1 and
                     sum(n['type']=='GtkButton' and n['label']=='Sicherungen wiederherstellen' for n in nodes)==1
                     for nodes in cards)
        global_auto='Für alle Endpunkte' in ui.labels() and sum(n['type']=='GtkSwitch' and n['mapped'] for n in ui.tree())==1
        ui.set_active('GtkSwitch',True);enabled=tomllib.loads(ui.cfg.read_text())['settings']['auto_sync']
        ui.set_active('GtkSwitch',False);paused=not tomllib.loads(ui.cfg.read_text())['settings']['auto_sync']
        click_in_card('Ziel 1','Verbindung testen')
        first_ok=eventually(lambda:any('✓' in n['label'] and str(targets[0]) in n['label'] for n in card_nodes('Ziel 1')))
        click_in_card('Ziel 2','Verbindung testen')
        second_error=eventually(lambda:any(str(targets[1]) in n['label'] and ':' in n['label'] and '✓' not in n['label'] for n in card_nodes('Ziel 2') if n['type']=='GtkLabel' and n['label'].startswith('Lokal:')))
        first_still_ok=any('✓' in n['label'] for n in card_nodes('Ziel 1'))
        click_in_card('Ziel 2','Sicherungen wiederherstellen')
        second_restore=eventually(lambda:any('Sicherungsziel nicht gefunden' in text and str(targets[1]) in text for text in ui.labels()))
        back=ui.find('AdwBackButton');button=next(n for n in ui.descendants(back) if n['type']=='GtkButton');ui.command('click',button['id']);time.sleep(.4)
        click_in_card('Ziel 1','Sicherungen wiederherstellen')
        first_restore=eventually(lambda:any('1 Sicherungsdatei gefunden' in text for text in ui.labels()))
        found={'Archive', 'old.txt'}.issubset(ui.labels())
        back=ui.find('AdwBackButton');button=next(n for n in ui.descendants(back) if n['type']=='GtkButton');ui.command('click',button['id']);time.sleep(.4)
        ui.click('Wiederherstellen')
        first_restore=first_restore and eventually(lambda:'old.txt' in ui.labels())
        targets[1].unlink()
        targets[1].mkdir()
        dropdown=ui.find('GtkDropDown')
        ui.command('selected', dropdown['id'], 1)
        empty=eventually(lambda:'Keine Sicherungsdateien gefunden.' in ui.labels())
        assert empty and ui.find('GtkButton','Erneut laden')['sensitive']
        assert not ui.find('GtkButton','Wiederherstellungsordner wählen …')['sensitive']
        record('UI-33','Separate endpoint cards keep add action outside and scope connection/restore actions',
               separate and global_auto and enabled and paused and first_ok and second_error and first_still_ok and second_restore and first_restore and found,
               f'Separate cards={separate}; global auto={global_auto and enabled and paused}; tests isolated={first_ok and second_error and first_still_ok}; restore isolated={second_restore and first_restore and found}',
               'One card per endpoint; add outside; global auto is explicit; each card tests and restores only its own target',ui.snapshot('scoped-restore'))
    finally:ui.stop()


def endpoint_removal():
    app = UI('endpoint-removal')
    source = app.root/'Dokumente'
    source.mkdir()
    file = source/'Probe.txt'
    file.write_text('first backup')
    targets = [app.root/'backup-one', app.root/'backup-two']
    app.write_config([source], targets)

    def open_removal(title):
        heading = app.find('GtkLabel', title)
        header = app.nodes[heading['parent']]
        button = next(node for node in app.descendants(header) if node['type'] == 'GtkButton')
        app.command('click', button['id'])

    try:
        app.launch()
        run('xdotool', 'windowsize', str(app.xid), '1180', '940')
        time.sleep(.4)
        app.click('Jetzt synchronisieren')
        assert app.idle()
        copies = [target/'Dokumente/Probe.txt' for target in targets]
        assert all(copy.read_text() == 'first backup' for copy in copies)
        evidence = app.snapshot('buttons')
        open_removal('Ziel 1')
        assert eventually(lambda: any('Vorhandene Sicherungen am Ziel bleiben erhalten.' in label
                                      for label in app.labels()))
        app.click('Abbrechen')
        assert len(tomllib.loads(app.cfg.read_text())['targets']) == 2
        open_removal('Ziel 1')
        app.click('Endpunkt entfernen')
        assert eventually(lambda: 'Ziel' in app.labels())
        app.expand('Sicherungsdetails')
        assert '1' in app.labels()
        assert tomllib.loads(app.cfg.read_text())['targets'] == [{'type': 'local', 'path': str(targets[1])}]

        app.stop()
        app.launch()
        app.set_active('GtkSwitch', True)
        assert eventually(lambda: any('Live-Sync aktiv' in label for label in app.labels()))
        file.write_text('second backup')
        assert eventually(lambda: copies[1].read_text() == 'second backup', 12)
        assert app.idle()
        assert [copy.read_text() for copy in copies] == ['first backup', 'second backup']
        open_removal('Ziel')
        app.click('Endpunkt entfernen')
        assert eventually(lambda: 'Kein Endpunkt konfiguriert' in app.labels())
        assert not app.find('GtkButton', 'Jetzt synchronisieren', mapped=False)['sensitive']
        assert not tomllib.loads(app.cfg.read_text())['targets']
        assert all(copy.is_file() for copy in copies)
        record('UI-41', 'Remove an endpoint without deleting its backups', True,
               'Cancel keeps both targets; after restart live sync updates only the remaining target',
               'Removing the final endpoint disables sync and preserves both backups', evidence)
    finally:
        app.stop()



def native_header_layout():
    ui=UI('native-header');source,targets=fixture(ui)
    settings=ui.root/'config/gtk-4.0/settings.ini';settings.parent.mkdir(parents=True)
    settings.write_text('[Settings]\ngtk-decoration-layout=icon,close:minimize,maximize\n')
    try:
        ui.launch();ui.idle()
        header=ui.find('AdwHeaderBar');nodes=ui.descendants(header)
        controls=[n for n in nodes if n['type']=='GtkWindowControls' and n['mapped']]
        button_counts=[sum(n['type']=='GtkButton' and n['mapped'] for n in ui.descendants(control)) for control in controls]
        icons=[]
        for control in controls:
            for n in ui.descendants(control):
                if n['type'] not in ('GtkImage','GtkWindowIcon') or not n['mapped']:continue
                parent=n['parent'];inside_button=False
                while parent!=control['id'] and parent>=0:
                    if ui.nodes[parent]['type']=='GtkButton':inside_button=True
                    parent=ui.nodes[parent]['parent']
                if not inside_button:icons.append(n)
        brand=sum(n['type']=='GtkDrawingArea' and n['mapped'] for n in nodes)
        record('UI-34','Native icon decoration does not duplicate the brand and preserves left/right controls',
               button_counts==[1,2] and not icons and brand==1,
               f'Left/right buttons={button_counts}; extra native icons={len(icons)}; brand marks={brand}',
               'Desktop icon/menu decoration removed; close remains left and minimize/maximize remain right',ui.snapshot('native-icon-layout'))
    finally:ui.stop()


def exclusion_folder_picker():
    ui=UI('exclusion-folder');source,targets=fixture(ui)
    selected=source/'Cache [old]';selected.mkdir();(selected/'private.bin').write_bytes(b'not backed up')
    similar=source/'Cache o';similar.mkdir();(similar/'keep.bin').write_bytes(b'keep this backup')
    outside=ui.root/'not-a-source';outside.mkdir()
    def choose(path):
        open_exclusions(ui);ui.click('Ordner auswählen …')
        run('xdotool','key','ctrl+l');time.sleep(.2)
        entry=ui.find('GtkFileChooserEntry');ui.command('text',entry['id'],str(path));run('xdotool','key','Return')
        if not eventually(lambda:not any(n['type']=='GtkFileChooserDialog' and n['mapped'] for n in ui.tree()),3):ui.click('_Select')
        time.sleep(.3)
    try:
        ui.launch();ui.idle();ui.click('Einstellungen')
        choose(outside)
        rejected=any('außerhalb' in text for text in ui.labels()) and not tomllib.loads(ui.cfg.read_text())['settings'].get('exclude_patterns')
        choose(selected)
        patterns=tomllib.loads(ui.cfg.read_text())['settings'].get('exclude_patterns',[])
        ui.click('Jetzt synchronisieren');ui.idle()
        exact=all(not (target/'Documents/Cache [old]/private.bin').exists() and
                  (target/'Documents/Cache o/keep.bin').read_bytes()==b'keep this backup' for target in targets)
        record('UI-35','Choose an excluded folder without typing patterns and escape literal names',rejected and patterns==['Cache [[]old[]]/**'] and exact,
               f'Outside path rejected={rejected}; saved={patterns}; exact folder excluded in both targets={exact}',
               'Picker creates a relative subtree rule; glob characters in folder names stay literal; unrelated data is still backed up',ui.snapshot('folder-rule'))
    finally:ui.stop()


def recovery_choose(ui, path, button='Separaten Ordner wählen …', cancel=False):
    ui.click(button)
    if cancel:
        ui.click('_Cancel');return
    run('xdotool','key','ctrl+l');time.sleep(.2)
    entry=ui.find('GtkFileChooserEntry');ui.command('text',entry['id'],str(path));run('xdotool','key','Return')
    if not eventually(lambda:not any(n['type']=='GtkFileChooserDialog' and n['mapped'] for n in ui.tree()),3):ui.click('_Select')


def enter_existing(ui, target, recovery=True):
    if recovery:
        ui.click('Vorhandene Sicherung wiederherstellen')
    else:
        ui.click('Einrichtung starten')
        for label in ('Bilder','Downloads'):ui.set_active('GtkCheckButton',True,label)
        ui.click('Weiter')
    choose_native_folder(ui,target);ui.click('Weiter');ui.set_active('GtkSwitch',False)
    ui.click('Endpunkt prüfen & einrichten' if recovery else 'Sync einrichten & starten')
    assert eventually(lambda:'Bestehende Sicherung gefunden' in ui.labels())


def guided_recovery():
    ui=UI('reinstall-originals',home_dirs=('Bilder','Downloads'))
    backup=ui.root/'StorageBox';(backup/'Bilder').mkdir(parents=True);(backup/'Downloads').mkdir()
    (backup/'Bilder/photo.jpg').write_bytes(b'old photo');(backup/'Downloads/install.iso').write_bytes(b'old download')
    try:
        ui.launch();enter_existing(ui,backup,recovery=False)
        cfg=tomllib.loads(ui.cfg.read_text())
        paused=not cfg['settings']['auto_sync'] and cfg['settings']['pending_recovery']['path']==str(backup)
        untouched=not any((ui.root/'home/Bilder').iterdir()) and (backup/'Bilder/photo.jpg').read_bytes()==b'old photo'
        ui.cfg.write_text(ui.cfg.read_text().replace('sync_mode = "backup"','sync_mode = "mirror"'))
        ui.snapshot('found-before-sync');ui.stop();ui.launch()
        resumed=eventually(lambda:'Bestehende Sicherung gefunden' in ui.labels()) and not any((ui.root/'home/Bilder').iterdir())
        ui.click('Ausgewählte Ordner wiederherstellen')
        complete=eventually(lambda:'Zur Übersicht' in ui.labels())
        cfg=tomllib.loads(ui.cfg.read_text())
        restored=(ui.root/'home/Bilder/photo.jpg').read_bytes()==b'old photo' and (ui.root/'home/Downloads/install.iso').read_bytes()==b'old download'
        safe=cfg['settings']['sync_mode']=='backup' and cfg['settings']['auto_sync'] and 'pending_recovery' not in cfg['settings']
        record('UI-36','Whole-PC recovery is offered before sync and survives restart in mirror mode',paused and untouched and resumed and complete and restored and safe,
            f'Paused={paused}; untouched before choice={untouched}; restart resumed={resumed}; whole folders restored={restored}; backup mode after recovery={safe}',
            'No upload or mirror before recovery; original home folders restored without per-file selection; completed setup resumes in Backup mode',ui.snapshot('originals-complete'))
        ui.click('Zur Übersicht');ui.idle()
    finally:ui.stop()


def separate_recovery():
    ui=UI('reinstall-separate')
    backup=ui.root/'StorageBox';(backup/'Bilder').mkdir(parents=True);(backup/'Bilder/photo.jpg').write_bytes(b'old photo')
    output=ui.root/'separate';output.mkdir()
    try:
        ui.launch();enter_existing(ui,backup)
        ui.set_active('GtkCheckButton',True,'In einen separaten Ordner wiederherstellen')
        blocked=not ui.find('GtkButton','Ausgewählte Ordner wiederherstellen')['sensitive']
        recovery_choose(ui,output,cancel=True)
        cancelled=not ui.find('GtkButton','Ausgewählte Ordner wiederherstellen')['sensitive']
        recovery_choose(ui,output)
        ui.click('Ausgewählte Ordner wiederherstellen');complete=eventually(lambda:'Zur Übersicht' in ui.labels())
        cfg=tomllib.loads(ui.cfg.read_text())
        correct=(output/'Bilder/photo.jpg').read_bytes()==b'old photo' and not (ui.root/'home/Bilder').exists() and not cfg['folders'] and not cfg['settings']['auto_sync']
        record('UI-37','Separate-folder recovery restores all files and leaves automatic sync paused',blocked and cancelled and complete and correct,
            f'Needs chosen folder={blocked}; cancel safe={cancelled}; output and config safe={correct}',
            'Explicit separate destination; whole folders restored; extraction does not enable uploads or create home sources',ui.snapshot('separate-complete'))
    finally:ui.stop()


def catalog_recovery():
    ui=UI('reinstall-catalog')
    backup=ui.root/'StorageBox';(backup/'Projekt').mkdir(parents=True);(backup/'.cloudsave').mkdir()
    (backup/'Projekt/save.txt').write_text('portable nested data')
    (backup/'.cloudsave/folders.toml').write_text('version=1\n[[folders]]\nnamespace="Projekt"\nhome_relative="Arbeit/Projekt"\n[[folders]]\nnamespace="Leer"\nhome_relative="Leer"\n')
    try:
        ui.launch();enter_existing(ui,backup)
        mapped=str(ui.root/'home/Arbeit/Projekt') in ui.labels()
        ui.snapshot('portable-mapping')
        ui.click('Ausgewählte Ordner wiederherstellen');complete=eventually(lambda:'Zur Übersicht' in ui.labels())
        cfg=tomllib.loads(ui.cfg.read_text())
        correct=(ui.root/'home/Arbeit/Projekt/save.txt').read_text()=='portable nested data' and (ui.root/'home/Leer').is_dir()
        aliases=cfg['settings']['folder_names'].get(str(ui.root/'home/Arbeit/Projekt'))=='Projekt' and cfg['settings']['folder_home_paths'].get(str(ui.root/'home/Arbeit/Projekt'))=='Arbeit/Projekt'
        ui.click('Zur Übersicht');ui.idle()
        stable=not (backup/'Arbeit').exists() and (backup/'Projekt/save.txt').read_text()=='portable nested data'
        record('UI-38','Portable catalog restores nested and empty source folders with stable remote names',mapped and complete and correct and aliases and stable,
            f'Preview mapping={mapped}; nested/empty restored={correct}; aliases persisted={aliases}; remote namespace stable={stable}',
            'Saved home-relative paths restored; metadata is not a user file; fresh sync preserves backup namespaces',ui.snapshot('catalog-dashboard'))
    finally:ui.stop()


def recovery_conflict_and_failure():
    ui=UI('reinstall-conflict',home_dirs=('Bilder',))
    backup=ui.root/'StorageBox';(backup/'Bilder').mkdir(parents=True)
    (backup/'Bilder/a.txt').write_text('old a');(backup/'Bilder/b.txt').write_text('old b')
    source=ui.root/'home/Bilder';(source/'a.txt').write_text('keep local a')
    outside=ui.root/'outside';outside.mkdir()
    source.rename(ui.root/'original-bilder');source.symlink_to(outside,target_is_directory=True)
    try:
        ui.launch();enter_existing(ui,backup)
        ui.click('Ausgewählte Ordner wiederherstellen')
        rejected=eventually(lambda:any('Wiederherstellung nicht abgeschlossen' in label for label in ui.labels())) and not any(outside.iterdir())
        cfg=tomllib.loads(ui.cfg.read_text());pending=not cfg['settings']['auto_sync'] and 'pending_recovery' in cfg['settings']
        source.unlink();(ui.root/'original-bilder').rename(source)
        ui.click('Ausgewählte Ordner wiederherstellen');complete=eventually(lambda:'Zur Übersicht' in ui.labels())
        cfg=tomllib.loads(ui.cfg.read_text())
        preserved=(source/'a.txt').read_text()=='keep local a' and (source/'b.txt').read_text()=='old b' and (backup/'Bilder/a.txt').read_text()=='old a' and not cfg['settings']['auto_sync']
        record('UI-39','Failed recovery stays paused; retry retains local conflicts and remote backups',rejected and pending and complete and preserved,
            f'Symlink rejected={rejected}; pending persisted={pending}; retry complete={complete}; local and backup retained={preserved}',
            'No unsafe path writes; failure never starts mirroring; existing local files retained and conflicts keep Live-Sync paused',ui.snapshot('conflicts-complete'))
    finally:ui.stop()



def folder_removal():
    app = UI('folder-removal')
    source = app.root/'Dokumente'
    other = app.root/'Bilder'
    targets = [app.root/'backup-one', app.root/'backup-two']
    for path in [source, other]:
        path.mkdir()
        (path/'fixture.txt').write_text(path.name)
    app.write_config([source, other], targets)

    def remove_button(name):
        label = app.find('GtkLabel', name)
        row = app.nodes[app.nodes[label['parent']]['parent']]
        button = next(node for node in app.descendants(row) if node['type'] == 'GtkButton')
        app.command('click', button['id'])

    try:
        app.launch(); app.click('Jetzt synchronisieren'); assert app.idle()
        remove_button('Bilder')
        assert eventually(lambda: 'Bilder' not in app.labels())
        assert all((target/'Bilder/fixture.txt').is_file() for target in targets)

        app.set_active('GtkSwitch', True)
        app.set_active('GtkCheckButton', False)
        remove_button('Dokumente')
        app.click('Abbrechen')
        assert 'Dokumente' in app.labels()
        assert all((target/'Dokumente/fixture.txt').is_file() for target in targets)
        remove_button('Dokumente')
        app.click('Ordner und Kopien entfernen')
        assert eventually(lambda: all(not (target/'Dokumente').exists() for target in targets))
        assert app.idle()
        assert 'Dokumente' not in app.labels()
        config = tomllib.loads(app.cfg.read_text())
        assert not config['folders'] and not config['settings'].get('pending_folder_removals')
        assert all((path/'fixture.txt').is_file() for path in (source, other))
        assert all((target/'Bilder/fixture.txt').is_file() for target in targets)
        record('UI-40', 'Remove tracked folders safely from the dashboard', True,
               'Cancel preserves both copies; accepted live removal deletes both target folders',
               'Paused sync preserves backups and both local originals remain byte-for-byte')
    finally:
        app.stop()


def main():
    global OUT, BINARY
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    parser.add_argument('--binary',type=pathlib.Path,help='Test this native build instead of an installed Flatpak')
    args=parser.parse_args();OUT=args.output.resolve();OUT.mkdir(parents=True,exist_ok=False)
    BINARY=args.binary.resolve() if args.binary else None
    if not Gdk.Display.get_default(): raise RuntimeError('Reachable X11/XWayland display required')
    run('cc','-std=c11','-shared','-fPIC','-Wall','-Wextra','-Werror','-O2','-o',str(HERE/'gtk_qa_bridge.so'),str(HERE/'gtk_qa_bridge.c'),'-ldl')
    if BINARY:
        build=dict(mode='native',binary=str(BINARY))
        binary=BINARY
    else:
        location=run('flatpak','info','--show-location',APP)
        build=dict(mode='installed-flatpak',flatpak_commit=run('flatpak','info','--show-commit',APP),flatpak_info=run('flatpak','info',APP))
        binary=pathlib.Path(location,'files/bin/cloudsave')
    metadata=dict(app=APP,**build,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        timestamp=run('date','--iso-8601=seconds'),method='GTK-main-thread widget actions via test-only preload bridge; real sync engine and filesystem',
        limitations=['X11 test session instead of native Wayland','No session bus, desktop portals, Secret Service or external network',
                     'Real Flatpak portal grants/cancel and authenticated WebDAV E2E deliberately excluded',
                     'Widget activation/property changes, not physical mouse/keyboard or accessibility certification'])
    (OUT/'environment.json').write_text(json.dumps(metadata,indent=2,ensure_ascii=False))
    try:
        for test in (onboarding,dashboard,endpoint_cards,missing_source,empty_targets,sync_status):test()
        empty_or_missing_home()
        unavailable_keyring()
        backup_mode()
        exclusion_settings()
        automatic_changes()
        folder_removal()
        endpoint_removal()
        if BINARY:restore_ui()
        if BINARY:native_picker()
        if BINARY:native_header_layout()
        if BINARY:exclusion_folder_picker()
        if BINARY:
            for test in (guided_recovery,separate_recovery,catalog_recovery,recovery_conflict_and_failure):test()
    except Exception:
        (OUT/'harness-error.txt').write_text(traceback.format_exc());traceback.print_exc();return 2
    total=len(RESULTS);failed=sum(r['status']=='FAIL' for r in RESULTS)
    summary=dict(total=total,passed=total-failed,failed=failed)
    (OUT/'summary.json').write_text(json.dumps(summary,indent=2))
    print(json.dumps(summary),flush=True)
    return int(bool(failed))

if __name__=='__main__':sys.exit(main())
