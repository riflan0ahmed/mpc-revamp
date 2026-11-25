# Builds website/keyboard-shortcuts/index.html from the app's own command table, so the page lists
# exactly the default bindings MPC-RV ships with. Run it from the repository root after changing
# mpc-rv/core/src/commands.rs or adding a command to the Linux app:
#
#   python3 website/make_shortcuts.py
#
# A command is listed when the Linux app implements it (its action name appears in mpc-rv/linux/src/,
# where ui.rs registers it), it isn't a hidden DVD command (commands::is_dvd), and its key isn't
# already taken by an earlier command (the first match wins, as in the app).
import glob, html, re, sys

SITE = "https://riflan0ahmed.github.io/mpc-revamp"
src = open("mpc-rv/core/src/commands.rs").read()
app = "".join(open(f).read() for f in glob.glob("mpc-rv/linux/src/*.rs"))
cmds = re.findall(r'c\((\d+), "([^"]+)", "([^"]+)", "([^"]*)", (\w+), (\w+), (\w+)\)', src)
if len(cmds) < 150:
    sys.exit(f"only {len(cmds)} commands found in commands.rs; has its format changed?")


def is_dvd(a):  # same rule as commands::is_dvd
    return a == "file-opendvdbd" or a.startswith("dvd-") or (a.startswith("navigate-") and "menu" in a)


MODS = {"Control": "Ctrl", "Alt": "Alt", "Shift": "Shift", "Super": "Super"}
KEYS = {"space": "Space", "period": ".", "Return": "Enter", "Page_Up": "Page Up", "Page_Down": "Page Down",
        "KP_Add": "Num +", "KP_Subtract": "Num -"}


def key_label(accel):
    mods = [MODS[m] for m in re.findall(r"<(\w+)>", accel)]
    k = re.sub(r"<\w+>", "", accel)
    k = KEYS.get(k) or (f"Num {k[3:]}" if k.startswith("KP_") else k.upper() if len(k) == 1 else k)
    return "+".join(mods + [k])


# Plain names for MPC's terse labels (what the command does in MPC-RV; see ui.rs).
RENAME = {"panscan-rotatexm": "Flip upside down", "panscan-rotateym": "Mirror",
          "panscan-rotatezp": "Rotate 90° counter-clockwise", "panscan-rotatezm": "Rotate 90° clockwise",
          "view-vsync": "Sync video to display (on/off)", "view-fullscreen-secondary": "Fullscreen (keep resolution)"}
WORDS = [("PnS ", "Pan & scan: "), ("Inc ", "Increase "), ("Dec ", "Decrease "), ("Goto ", "Go to "),
         ("Prev ", "Previous "), ("Caption&Menu", "Caption & Menu")]


def plain(action, label):
    if action in RENAME:
        return RENAME[action]
    label = label.rstrip(".")
    for a, b in WORDS:
        label = label.replace(a, b)
    return label


MOUSE = {"LUp": "Left click", "LDblClk": "Double-click", "RUp": "Right click", "WUp": "Wheel up",
         "WDown": "Wheel down", "X1Down": "Back button", "X2Down": "Forward button"}
MEDIA = {"PlayPause": "Play/Pause key", "Play": "Play key", "Pause": "Pause key", "Stop": "Stop key",
         "Next": "Next key", "Previous": "Previous key"}

GROUPS = [  # (title, id, action prefixes)
    ("Playback", "playback", ["play"]),
    ("Files and subtitles", "files", ["file"]),
    ("Navigation", "navigation", ["navigate", "goto", "favorites"]),
    ("Audio and subtitle tracks", "tracks", ["stream", "sub", "subtitles"]),
    ("Volume", "volume", ["volume", "normalize", "regain"]),
    ("View and window", "view", ["view", "d3dfullscreen", "ontop"]),
    ("Zoom, pan and aspect ratio", "video", ["panscan", "aspectratio"]),
    ("Other", "other", []),
]


def group_of(action):
    prefix = action.split("-")[0]
    return next((g for g in GROUPS if prefix in g[2]), GROUPS[-1])


rows = {g[1]: [] for g in GROUPS}
taken, skipped = set(), []
mouse_rows, media_rows = [], []
for _id, action, label, key, mouse, _mouse_fs, media in cmds:
    if is_dvd(action) or f'"{action}"' not in app:
        if key:
            skipped.append(action)
        continue
    name = plain(action, label)
    if key and key not in taken:
        taken.add(key)
        rows[group_of(action)[1]].append((name, key_label(key)))
    if mouse in MOUSE:
        mouse_rows.append((name, MOUSE[mouse]))
    if media in MEDIA:
        media_rows.append((name, MEDIA[media]))
count = sum(len(r) for r in rows.values())

e = html.escape


def table(items, head):
    trs = "\n".join(f"        <tr><td>{e(n)}</td><td><kbd>{e(k)}</kbd></td></tr>" for n, k in items)
    return f"""      <table class="keys">
        <tr><th>Command</th><th>{head}</th></tr>
{trs}
      </table>"""


sections = [(t, i, table(rows[i], "Key")) for t, i, _ in GROUPS if rows[i]]
sections.append(("Mouse", "mouse", table(mouse_rows, "Button")))
sections.append(("Media keys", "media", table(media_rows, "Key")))
toc = "\n".join(f'      <li><a href="#{i}">{e(t)}</a></li>' for t, i, _ in sections)
body = "\n".join(f"""    <section class="keys-section" id="{i}">
      <h2>{e(t)}</h2>
{tb}
    </section>""" for t, i, tb in sections)

page = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>MPC-HC Keyboard Shortcuts on Linux | MPC-RV</title>
<meta name="description" content="All {count} default keyboard shortcuts in MPC-RV, the free MPC-HC-style video player for Linux: playback, seeking, subtitles, audio, zoom and pan &amp; scan.">
<link rel="canonical" href="{SITE}/keyboard-shortcuts/">
<meta name="theme-color" content="#16181c">
<link rel="icon" type="image/png" sizes="48x48" href="../icon-48.png">
<link rel="apple-touch-icon" href="../icon-256.png">
<meta property="og:type" content="article">
<meta property="og:site_name" content="MPC-RV">
<meta property="og:title" content="MPC-HC keyboard shortcuts on Linux (MPC-RV)">
<meta property="og:description" content="Every default shortcut in MPC-RV, the free MPC-HC-style video player for Linux.">
<meta property="og:url" content="{SITE}/keyboard-shortcuts/">
<meta property="og:image" content="{SITE}/og-image.jpg">
<meta name="twitter:card" content="summary_large_image">
<link rel="stylesheet" href="../style.css">
</head>
<body>
<!-- Generated by website/make_shortcuts.py from mpc-rv/core/src/commands.rs. Don't edit by hand. -->

<header>
  <div class="wrap">
    <a class="brand" href="../"><img src="../icon-48.png" width="32" height="32" alt="">MPC-RV</a>
    <nav>
      <a class="optional" href="../#features">Features</a>
      <a href="../#install">Install</a>
      <a href="https://github.com/riflan0ahmed/mpc-revamp">GitHub</a>
    </nav>
  </div>
</header>

<main>
  <div class="wrap">
    <p class="crumb"><a href="../">MPC-RV</a> › Keyboard shortcuts</p>
    <div class="page-head">
      <h1>MPC-HC keyboard shortcuts in MPC-RV</h1>
      <p class="intro">MPC-RV, the free MPC-HC-style video player for Linux, ships with MPC-HC's default
        shortcuts. These are all {count} of them, plus the mouse and media keys. Every one can be changed in
        <strong>Options › Keys</strong>, and the player shows them in its menus too.</p>
    </div>
    <ul class="toc">
{toc}
    </ul>
{body}
    <p class="intro" style="margin-top: 32px">Missing a shortcut you used in MPC-HC? Some commands, like
      downloading subtitles or Always on Top, aren't in MPC-RV yet.
      <a href="https://github.com/riflan0ahmed/mpc-revamp/issues">Ask for it on GitHub</a>.</p>
  </div>
</main>

<footer>
  <div class="wrap">
    <p>MPC-RV is free software under the GPL-3.0-or-later license ·
      <a href="https://github.com/riflan0ahmed/mpc-revamp">Source code</a> ·
      <a href="https://github.com/riflan0ahmed/mpc-revamp/issues">Report a bug</a></p>
    <p>Made by <a href="https://github.com/riflan0ahmed">Riflan Ahmed</a> ·
      <a href="https://github.com/riflan0ahmed/mpc-revamp/releases">Release notes</a></p>
    <p>Thanks to the MPC-HC developers for the original player. MPC-RV isn't affiliated with them.</p>
  </div>
</footer>

</body>
</html>
"""
import os
os.makedirs("website/keyboard-shortcuts", exist_ok=True)
open("website/keyboard-shortcuts/index.html", "w").write(page)
print(f"{count} keys, {len(mouse_rows)} mouse, {len(media_rows)} media; not in the app yet: {', '.join(skipped)}")
