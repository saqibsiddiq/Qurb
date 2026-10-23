#!/usr/bin/env python3
"""Every page, pop-up, message and notification in the Android app and the
desktop window, with its words, written to docs/design/inventory.md.

    python3 scripts/design-inventory.py

Reads the source and nothing else. Words are found as string literals in the
interface code (Kotlin, the window's JS and HTML) and in the Rust that sends
words to a person (desktop notifications, the tray, commands' errors, the
phone's engine); each is filed under the function it is in, and that function
under a place. What it cannot do is listed under "Known gaps" in the output.
"""
import json, re, sys, pathlib, subprocess
root = pathlib.Path(__file__).resolve().parent.parent
out = []
skip_line = re.compile(r'Log\.[dewiv]\(|tracing::|println|getString\(|putString\(|getBoolean\(|putBoolean\(|getLong\(|putLong\(|getInt\(|putInt\(|Intent\(|\.action ==|setAction|"android\.|import |package |@Suppress|console\.(log|error|warn)|invoke\(|addEventListener|querySelector|getElementById|classList|dataset|setAttribute\(|createElement\(|\.contains\(\"|startsWith\(|endsWith\(|mimeType|"image/|"video/|"audio/|"text/|"application/')
lit = re.compile(r'"((?:[^"\\]|\\.)*)"|`((?:[^`\\]|\\.)*)`|\'((?:[^\'\\]|\\.)*)\'')
def words(s):
    s2 = re.sub(r'\$\{[^}]*\}|\$[A-Za-z_][A-Za-z0-9_.]*', 'X', s)
    return bool(re.search(r'[A-Za-z]{2,}', s2)) and (' ' in s2.strip() or re.match(r'^[A-Z][a-z]+[.!?…]?$', s2) or re.search(r'[.!?…:]$', s2))
for path in sorted(list((root/'android/app/src/main/java/com/qurb').glob('*.kt')) + list((root/'crates/desktop/ui').glob('*.js')) + [root/'crates/desktop/ui/index.html']):
    for n, line in enumerate(path.read_text().splitlines(), 1):
        t = line.strip()
        if t.startswith(('//', '*', '/*', '#')) or skip_line.search(line): continue
        for m in lit.finditer(line):
            s = next(g for g in m.groups() if g is not None)
            if path.suffix == '.kt' and m.group(3) is not None: continue  # Kotlin chars
            if len(s) < 3 or not words(s): continue
            if re.search(r'^[a-z_\-]+(\.[a-z_\-]+)+$|^[a-z][a-z0-9_]*$|^#|^\.[a-z]|=>|\bfunction\b|^\w+://', s): continue
            out.append((str(path.relative_to(root)), n, s))
# Android layouts and menus: words written into the XML rather than the code.
res = root/'android/app/src/main/res'
for path in sorted(list((res/'layout').glob('*.xml')) + list((res/'menu').glob('*.xml'))):
    for n, line in enumerate(path.read_text().splitlines(), 1):
        for m in re.finditer(r'android:(?:text|hint|contentDescription|title)="([^"@][^"]*)"', line):
            out.append((str(path.relative_to(root)), n, m.group(1)))
html = (root/'crates/desktop/ui/index.html').read_text()
for n, line in enumerate(html.splitlines(), 1):
    for m in re.finditer(r'>([^<>{}]*[A-Za-z]{2,}[^<>{}]*)<', line):
        s = m.group(1).strip()
        if s: out.append(('crates/desktop/ui/index.html', n, s))
    for m in re.finditer(r'(?:aria-label|title|placeholder)="([^"]+)"', line):
        out.append(('crates/desktop/ui/index.html', n, m.group(1)))

rust = ['crates/desktop/src/notify.rs','crates/tray/src/ui.rs','crates/desktop/src/commands.rs','crates/mobile-ffi/src/lib.rs','crates/desktop/src/main.rs']
lit_rs = re.compile(r'"((?:[^"\\]|\\.)*)"')
skip_rs = re.compile(r'tracing::|debug!|info!|warn!|error!|trace!|println!|eprintln!|#\[|expect\(|\.context\(|env::var|"--|\.join\("')
for f in rust:
    for n, line in enumerate((root/f).read_text().splitlines(), 1):
        if line.strip().startswith('//') or skip_rs.search(line): continue
        for m in lit_rs.finditer(line):
            s = m.group(1)
            if len(s) >= 4 and re.search(r'[A-Za-z]{2,}', s) and ' ' in s and not re.search(r'^\w+://|\{[a-z_]+:\?', s):
                out.append((f, n, s))
fn_kt = re.compile(r'^\s*(?:private |internal |override |suspend |inline )*fun\s+(?:[A-Za-z<>.,? ]+\.)?([A-Za-z_][A-Za-z0-9_]*)\s*\(')
fn_js = re.compile(r'^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)|^\s*(?:export\s+)?(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>')
fn_rs = re.compile(r'^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+([a-z_][a-z0-9_]*)')
cls_kt = re.compile(r'^\s*(?:private |internal )?(?:object|class)\s+([A-Za-z_][A-Za-z0-9_]*)')
_cache = {}
def enclosing(f, n):
    if f not in _cache: _cache[f] = (root/f).read_text().splitlines()
    L = _cache[f]; fn = cls = ''
    for i in range(n-1, -1, -1):
        if not fn:
            m = (fn_kt if f.endswith('.kt') else fn_rs if f.endswith('.rs') else fn_js).match(L[i])
            if m: fn = next(g for g in m.groups() if g)
        if f.endswith('.kt') and not cls:
            m = cls_kt.match(L[i])
            if m: cls = m.group(1)
        if fn and (cls or not f.endswith('.kt')): break
    return (cls + '.' if cls else '') + fn
rows = [[f, n, (pathlib.Path(f).stem if f.endswith('.xml') else enclosing(f, n)), s] for f, n, s in out]

src = {}
def lines(f):
    if f not in src: src[f] = (root/f).read_text().splitlines()
    return src[f]
fn_def = re.compile(r'^\s*(?:private |internal |override |suspend |inline |pub(?:\([a-z]+\))? |async |export )*(?:fun|fn|function)\s|^\s*(?:export\s+)?(?:const|let)\s+[A-Za-z_$][\w$]*\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>')
def body(f, n):
    L = lines(f); start = n - 1
    while start > 0 and not fn_def.match(L[start]): start -= 1
    end = n
    while end < len(L) and not fn_def.match(L[end]): end += 1
    return '\n'.join(L[start:end])
html_places = [(16,'Setup'),(135,'The window: sidebar and frame'),(166,'Home'),(191,'Files'),(212,'Private Vault'),(230,'Devices'),(241,'Storage'),(294,'Settings'),(463,'Activity'),(472,'Recently deleted'),(481,'The window: sidebar and frame')]
A = {'HomeScreen':'Home','FilesScreen':'Files and Private Vault','KeptScreen':"A phone's folder on a computer",'FoldersScreen':'Your folders','Backups':'Backups: WhatsApp and folders','Backup':'Backups: WhatsApp and folders','WhatsApp':'Backups: WhatsApp and folders','DevicesScreen':'Devices','ShowCode':'Add a device: show a code','ScanActivity':'Add a device: scan a code','Approval':'Pairing: approving a device','SettingsScreen':'Settings','Appearance':'Settings','ActivityScreen':'Activity','DeletedScreen':'Recently deleted','SetupActivity':'Setup','ShareActivity':'Sending into Qurb from another app','MainActivity':'The app frame: tabs, sending, asks, unlocking','ManageSpaceActivity':"Android's Manage space screen",'QurbDocumentsProvider':"Qurb in Android's Files app",'ScreenLock':'Screen-lock prompts','Notices':'Notifications','Transfers':'Notifications','SyncWorker':'Background sync','Errors':'Error messages','Words':'Shared words: sizes, times, counts','Kit':'Shared components','lib':'Messages from the engine'}
A_order = ['Setup','Home','Files and Private Vault','Recently deleted','Your folders','Backups: WhatsApp and folders',"A phone's folder on a computer",'Devices','Add a device: show a code','Add a device: scan a code','Pairing: approving a device','Settings','Activity','The app frame: tabs, sending, asks, unlocking','Sending into Qurb from another app','Notifications','Background sync','Screen-lock prompts',"Android's Manage space screen","Qurb in Android's Files app",'Error messages','Messages from the engine','Shared words: sizes, times, counts','Shared components','Behind the scenes']
D = {'setup':'Setup','home':'Home','files':'Files','devices':'Devices','settings':'Settings','core':'Shared components','app':'The window: sidebar and frame','icons':'Shared components','theme':'Shared components','notify':'Notifications','ui':'Tray menu','commands':'Messages from the engine','main':'Notifications'}
D_order = ['Setup','The window: sidebar and frame','Home','Activity','Files','Private Vault','Recently deleted','Devices','Storage','Settings','Notifications','Tray menu','Messages from the engine','Shared components']
def place(f, n, fn):
    stem = pathlib.Path(f).stem
    if f.endswith('.kt') or 'mobile-ffi' in f:
        return 'A', A.get(stem, 'Behind the scenes')
    if '/res/' in f:
        LAYOUTS = {'activity_setup': 'Setup', 'activity_share': 'Sending into Qurb from another app', 'activity_scan': 'Add a device: scan a code',
                   'screen_home': 'Home', 'screen_files': 'Files and Private Vault', 'card_device': 'Devices', 'activity_main': 'The app frame: tabs, sending, asks, unlocking', 'tabs': 'The app frame: tabs, sending, asks, unlocking'}
        return 'A', LAYOUTS.get(stem, 'Shared components')
    if f.endswith('index.html'):
        p = 'Setup'
        for start, name in html_places:
            if n >= start: p = name
        return 'D', p
    p = D.get(stem, 'Shared components')
    low = fn.lower()
    if stem == 'home' and 'activity' in low or stem == 'home' and 'happened' in low: p = 'Activity'
    if stem == 'files' and ('deleted' in low or 'restore' in low or 'forget' in low): p = 'Recently deleted'
    if stem == 'files' and 'vault' in low: p = 'Private Vault'
    if stem == 'settings' and ('storage' in low or 'limit' in low or 'part' in low or 'reclaim' in low or 'free' in low): p = 'Storage'
    return 'D', p
def kind(f, n, fn, s):
    line = lines(f)[n-1]
    if re.search(r'\b(say|toast)\(', line): return 'message'
    stem = pathlib.Path(f).stem
    if stem in ('Notices','Transfers','notify'): return 'notification'
    b = body(f, n)
    if re.search(r'\bsheet\(|\.sheet\(\)|\bpanel\(|Sheet\(', b): return 'pop-up'
    if stem == 'ui' and 'tray' in f: return 'menu'
    if f.endswith('.rs') and stem in ('commands','lib'): return 'message'
    return 'page'
groups = {}
for f, n, fn, s in rows:
    app, p = place(f, n, fn)
    k = kind(f, n, fn, s)
    key = (app, p, k, f, fn)
    g = groups.setdefault(key, [])
    if s not in [x[1] for x in g]: g.append((n, s))
def title(f, fn, items):
    for n, s in items:
        if re.search(r'\.header\(|header\(|<h[12]|\.title|title:', lines(f)[n-1]): return s
    for n, s in items:
        if s.rstrip().endswith('?'): return s
    return None
commit = subprocess.run(['git','-C',str(root),'rev-parse','--short','HEAD'],capture_output=True,text=True).stdout.strip()
out = []
w = out.append
w('# Every surface, as it is\n')
w(f'What a person can see in the Android app and the desktop window: every page, pop-up, message and notification, with its words as the code has them. Made on {__import__('datetime').date.today().isoformat()} from the code at `{commit}` by `scripts/design-inventory.py`, which reads the source and writes this file again, as the checklist for the redesign the owner asked for on 2026-10-20:\n')
w('> can u run figma, i would like to redesign our application again with your help, i want to go through every page, pop up, text, notfication\n')
w('Each item is numbered so that it can be ticked off: **A** for Android, **D** for the desktop window. Within a place, *page* is what is on the screen itself, *pop-up* is a sheet, dialog or panel, *message* is a short line that appears and goes (a snackbar or toast) or an error, and *notification* is the system\'s. Each line gives the words and where they are in the code.\n')
w('## Known gaps\n')
w('This list was made by a script, so:\n')
w('- **Words put together in code come out in pieces.** A sentence built from parts, such as a device\'s name and a count, shows as its fixed parts, with `${...}` or `{...}` where a value goes. Some sentences are split across several lines.')
w('- **A pop-up is recognised by the code that opens it.** Words in the same function as a sheet are listed under it, even when some of them belong to the page.')
w('- **A message is recognised by the line that shows it.** Words kept in a variable and shown a line or two later are filed as page text: *Could not settle that* on Android\'s Home is one.')
w('- **What Android or the desktop draws for itself is left out**: the system\'s permission prompts, the file picker, the share sheet\'s own rows, and the screen-lock prompt apart from its title.')
w('- **Shown only by the engine in rare cases**: the messages from the engine are listed, but when each one appears has not been worked out for every one.')
w('- **Icons, illustrations and motion are not listed.** This is words and surfaces only.\n')
num = {}
for app, order, label in (('A', A_order, 'Android'), ('D', D_order, 'The desktop window')):
    w(f'## {label}\n')
    places = [p for p in order if any(k[0]==app and k[1]==p for k in groups)] + sorted({k[1] for k in groups if k[0]==app and k[1] not in order})
    for i, p in enumerate(places, 1):
        w(f'### {app}{i}. {p}\n')
        j = 0
        for k_kind in ('page','pop-up','message','notification','menu'):
            for key in sorted([k for k in groups if k[0]==app and k[1]==p and k[2]==k_kind], key=lambda k: (k[3], min(n for n,_ in groups[k]))):
                items = sorted(groups[key])
                j += 1
                f, fn = key[3], key[4]
                t = title(f, fn, items) if k_kind == 'pop-up' else None
                name = f'{k_kind}' + (f': “{t}”' if t else '')
                fname = fn.split('.')[-1] or '(top level)'
                w(f'**{app}{i}.{j}** {name} — `{f.split("/")[-1]}`, `{fname}`\n')
                for n, s in items:
                    s2 = s.replace('\\n', ' ¶ ').replace('|', '\\|')
                    w(f'- {s2} <sub>{n}</sub>')
                w('')
open(root/'docs/design/inventory.md','w').write('\n'.join(out) + '\n')
from collections import Counter
kinds = Counter(k[2] for k in groups)
print(f"docs/design/inventory.md: {len(groups)} surfaces ({', '.join(f'{v} {k}' for k, v in kinds.most_common())})")
