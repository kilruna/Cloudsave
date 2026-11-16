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
from gi.repository import Gdk, GdkX11

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
    def __init__(self, name, folders=None, targets=None, **settings):
        self.name = name
        scratch = os.environ.get('TMPDIR')
        if not scratch: raise RuntimeError('Set TMPDIR to a task-owned scratch directory, not system /tmp')
        self.root = pathlib.Path(tempfile.mkdtemp(prefix='cloudsave-ui-'+name+'-', dir=scratch))
        os.chmod(self.root, 0o700)
        for d in ('config','state','data','cache','home','control'):
            (self.root/d).mkdir()
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
        return eventually(lambda:self.find('GtkButton','Jetzt synchronisieren')['sensitive']
                          or 'Endpunkt einrichten' in self.labels(),10)

    def snapshot(self, tag):
        self.tree()
        stem=f'{self.name}-{tag}'
        (OUT/(stem+'.tree.tsv')).write_text((self.root/'control/tree.tsv').read_text())
        if self.xid:
            display=Gdk.Display.get_default()
            window=GdkX11.X11Window.foreign_new_for_display(display,self.xid)
            pixbuf=Gdk.pixbuf_get_from_window(window,0,0,window.get_width(),window.get_height())
            if pixbuf: pixbuf.savev(str(OUT/(stem+'.png')),'png',[],[])
        return [stem+'.png',stem+'.tree.tsv']

    def stop(self):
        if self.proc and self.proc.poll() is None:
            os.killpg(self.proc.pid,signal.SIGTERM)
            try:self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid,signal.SIGKILL);self.proc.wait(timeout=5)
        if hasattr(self,'logfile'):self.logfile.close()
        if self.cfg.exists():
            (OUT/(self.name+'.config.toml')).write_text(self.cfg.read_text())

    def __enter__(self):
        try:return self.launch()
        except Exception:
            self.stop()
            raise
    def __exit__(self,*args):self.stop()


def onboarding():
    with UI('onboarding') as ui:
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
        record('UI-03','Default source selection',sum(n['active']==1 for n in checks)==2,
               f'{sum(n["active"]==1 for n in checks)} selected of {len(checks)} sources','Two default sources selected')
        for _ in range(2):
            checks=[n for n in ui.tree() if n['type']=='GtkCheckButton' and n['mapped'] and n['active']]
            ui.command('active',checks[0]['id'],0)
        record('UI-04','Empty selection blocks Next',not ui.find('GtkButton','Weiter')['sensitive'],
               f'Next sensitive={ui.find("GtkButton","Weiter")["sensitive"]}','Next disabled without sources')
        ui.set_active('GtkCheckButton',True)
        ui.click('Weiter')
        before=ui.snapshot('local-before-activate')
        local=ui.find('GtkCheckButton', 'Lokaler Ordner / USB-Laufwerk')
        assert local['active']==1
        ui.command('activate',local['id']);time.sleep(.4)
        next_enabled=ui.find('GtkButton','Weiter')['sensitive']
        roots=[n for n in ui.tree() if n['parent']==-1 and n['visible']]
        browse=[n for n in ui.nodes if n['type']=='GtkButton' and n['mapped'] and n['sensitive'] and n['label']=='Ordner auswählen …']
        # An explicit, reusable browse button satisfies this scenario without
        # depending on the initially-active radio firing a toggled signal.
        record('UI-05','Initially selected local target is actionable',bool(browse) or len(roots)>1 or bool(next_enabled),
               f'{len(roots)} visible top-level window(s), Next sensitive={next_enabled}, initial local radio active',
               'Activating local target offers directory selection or an explicit browse control', before+ui.snapshot('local-stuck'))
        ui.set_active('GtkCheckButton',True,'WebDAV (HTTPS)')
        record('UI-06','Empty WebDAV credentials block Next',not ui.find('GtkButton','Weiter')['sensitive'],
               f'Next sensitive={ui.find("GtkButton","Weiter")["sensitive"]}','Next disabled without required fields')
        for title,value in [('Benutzername','qa-user'),('Server-URL','not a url'),('Passwort','synthetic-qa-value')]:
            typ='AdwPasswordEntryRow' if title=='Passwort' else 'AdwEntryRow'
            n=ui.find(typ,title);ui.command('text',n['id'],value)
        validations=[]
        for url,valid in [('',False),('  ',False),('not a url',False),('http://example.com',False),('ftp://example.com',False),('https://example.com/dav',True)]:
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
        ui.click('Jetzt synchronisieren');ui.idle()
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
        ui.click('Weiter');controls.append(len(window_controls(ui)))
        ui.click('Ordner auswählen …');ui.click('_Cancel')
        states=[not ui.find('GtkButton','Weiter')['sensitive'],bool(ui.find('GtkButton','Ordner auswählen …')['sensitive'])]
        choose_native_folder(ui,first)
        states.extend([str(first) in ui.labels(),bool(ui.find('GtkButton','Weiter')['sensitive'])])
        ui.click('Ordner auswählen …');ui.click('_Cancel')
        states.extend([str(first) in ui.labels(),bool(ui.find('GtkButton','Weiter')['sensitive'])])
        choose_native_folder(ui,second)
        ui.set_active('GtkCheckButton',True,'WebDAV (HTTPS)')
        states.append(not ui.find('GtkButton','Weiter')['sensitive'])
        ui.set_active('GtkCheckButton',True,'Lokaler Ordner / USB-Laufwerk')
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
        ui.set_active('GtkCheckButton',True,'WebDAV (HTTPS)')
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
        for test in (onboarding,dashboard,missing_source,empty_targets):test()
        if BINARY:native_picker()
    except Exception:
        (OUT/'harness-error.txt').write_text(traceback.format_exc());traceback.print_exc();return 2
    total=len(RESULTS);failed=sum(r['status']=='FAIL' for r in RESULTS)
    summary=dict(total=total,passed=total-failed,failed=failed)
    (OUT/'summary.json').write_text(json.dumps(summary,indent=2))
    print(json.dumps(summary),flush=True)
    return int(bool(failed))

if __name__=='__main__':sys.exit(main())
