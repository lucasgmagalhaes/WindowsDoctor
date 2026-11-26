# WindowsDoctor

(Formerly *Windows Folder Map*. The internal identifier `com.windowsfoldermap.app` is kept so existing
installs and their data carry over.)

Desktop app (Tauri v2 with **Rust** + **React**) for looking after a Windows PC. It has four modules,
picked from the bar on the left:

| Module | What it does |
| --- | --- |
| **Disk map** | Shows **where disk space is being used** in each partition, from the drive root down to the last folder, so you can find what to delete. |
| **Cleanup** | Caches and temp files (also on a schedule), duplicate files, installed programs with their leftovers, restore point space and CompactOS. |
| **Diagnostics** | Reads the event logs and the hardware to explain black screens, blue screens, crashes, slowness and disk errors; network test, guided repairs and a live black screen monitor. |
| **Optimizations** | Reversible tweaks (display, power, games, storage, privacy), one-click profiles, boot time and everything that starts with Windows. |
| **History** | Every change the app made, with undo for tweaks and startup programs, and an optional System Restore point before the first change of each session. |

## Why sunburst/treemap instead of a graph

A node-and-edge graph turns unreadable at thousands of folders, and in it size is not
visual. Here area is proportional to size:

- **Sunburst**: each ring is one folder level. Large slices stand out right away and you can see the
  whole path from the drive to the deep folder at once.
- **Treemap**: the same data as nested rectangles. Good for spotting big individual files.
- **List** beside the chart, sorted largest first, with % of the parent folder and actions.
- **Largest files** tab: the 500 largest files on the scanned partition.

## Disk map

- Lists the partitions with used/free space.
- Parallel scan in Rust (rayon), with live progress and cancel.
- Navigate by clicking: click a folder to go in, click the centre / breadcrumb / `Backspace` to go up.
- **Cleanup hints** for known space hogs: `Windows.old`, `$Recycle.Bin`,
  `hiberfil.sys`, `Temp`, Windows Update cache, `node_modules`, package caches (npm, gradle, nuget…),
  WSL/Docker disks, `WinSxS` (with the right DISM command) and others.
- **Delete** to the Recycle Bin or permanently, with confirmation. The tree and totals update with no new scan.
- **Rescan** a single folder (⟳) after cleaning it by other means.
- Open any item in Explorer.
- Protections: the backend refuses to delete drive roots and anything inside `%SystemRoot%` (`C:\Windows`).
- Symlinks/junctions are ignored (no double counting or loops). OneDrive "online-only"
  files count as 0 bytes, since they do not take up local space.
- Files smaller than a configurable threshold (1 MB by default) are grouped per folder, which keeps
  memory low on partitions with millions of files.

### More views

- **O que mudou**: after each complete scan a summary of folders over 100 MB is saved per scanned place; the
  next scan lists what grew or shrank by 200 MB or more, showing the most specific folder for each change.
- **Por tipo**: space by file type (videos, games, installers, virtual disks…) including the small grouped
  files; a click on an extension filters the largest files list.
- **Save / open a scan** (`.wdscan`, compact binary) to look at it later without scanning again.
- **Move to another drive** (⇄ on a folder): copies it, checks the size, leaves a junction in its place so
  programs keep working, and undoes everything if a step fails. System folders cannot be moved.

## Cleanup

The *Limpeza* module (also reachable from the disk map sidebar). Finds and deletes, in one go, what Windows and
apps recreate on their own. It measures each category first and lets you pick:

- System: user and Windows `Temp` (files newer than 24 h are kept), Windows Update cache (the service is
  stopped while cleaning), Delivery Optimization, crash dumps/WER reports, CBS/DISM/Panther logs, thumbnail
  and icon cache, Recycle Bin.
- Browsers (Chrome, Edge, Brave, Opera, Vivaldi, Firefox): caches only; history, passwords and logins stay.
- Apps: Discord, VS Code, Slack, Teams, Spotify… caches; DirectX/NVIDIA/AMD/Intel/Steam shader caches.
- Installers: leftovers of NVIDIA/AMD/Intel driver installers.
- Development: npm, Yarn, pnpm, pip, NuGet HTTP, Go and Cargo caches.
- Off by default (marked *revisar*): Recycle Bin, `Windows.old` and other previous Windows installs, `DISM
  /StartComponentCleanup` (WinSxS), installers (`.exe`/`.msi`/`.msix`) in Downloads, and local package
  repositories (Gradle, NuGet, Maven, pnpm store).

Deletion is permanent; files in use are skipped and reported. System categories need admin; the page
has a button to restart the app elevated. The list of locations is fixed in `src-tauri/src/cleanup.rs`;
the frontend only sends category ids. After cleaning, the affected folders of the current scan are
updated without a full rescan.

Other tabs in the Cleanup module:

- **Scheduled cleanup**: a Task Scheduler task runs the app windowless (`--auto-cleanup`) daily, weekly or
  monthly and cleans only the safe categories; the result goes to the history.
- **Duplicados**: same size → head/tail hash → full hash → byte-by-byte check. System and program folders and
  cloud-only files are skipped; removed copies go to the Recycle Bin and one copy is always kept.
- **Programas**: installed programs with size, opened with their own uninstaller (MSI `/I` becomes `/X`), then
  exact-name leftover folders in the install location, AppData and ProgramData.
- **Sistema**: space used by restore points (delete all but the newest, set the limit) and CompactOS.

## Diagnostics

Runs a read-only PowerShell script (`crates/systools/src/diagnostics.ps1`) and evaluates the result in Rust.
Each check gets a status (OK / tip / warning / problem), what to do next and, when there is one, a button
that applies the matching optimization.

- **Black screen / display driver**: driver resets (`Display` 4101, `nvlddmkm` 13/14/153, AMD). A reset is
  the classic "screen goes black for ~2 seconds and comes back".
- Display driver age, or no driver at all (Microsoft Basic Display Adapter).
- Multiplane Overlay (MPO) enabled on NVIDIA, a known cause of black flicker.
- WHEA hardware errors, unexpected shutdowns (Kernel-Power 41), disk/NTFS errors and physical disk health.
- Free space on the system drive, memory use, uptime (and Fast Startup), apps that crashed in the last week.
- **Blue screens** (WER 1001) with the stop code name and its usual cause, plus the latest minidumps.
- Devices with an error code in Device Manager; SMART counters per disk (temperature, wear, power-on hours,
  uncorrected errors); temperatures (ACPI zones, `nvidia-smi`); failed Windows Update installs and pending
  reboots; battery wear on laptops.
- System summary: Windows, CPU, RAM, GPUs with driver and current mode, monitors with connection type and,
  over DDC/CI (read only), monitor model and firmware version. Nothing is ever written to the monitor.
- **Live black screen monitor**: while the app is open it checks the log every 10 s and records each display
  driver reset with the program that was in the foreground.
- **Network test**: router and internet loss/latency, DNS time and Wi-Fi signal, telling local network problems
  from provider problems.
- **Guided repairs**: SFC, DISM `/RestoreHealth`, `chkdsk /scan` with live output, and the memory test.
- **Copy report** (plain text) and **Export HTML** (standalone page) for support forums.

## Optimizations

Every tweak reads the real current state, so the switch is always right even if it was changed elsewhere.
Turning a switch off applies the value described for that tweak; it may not restore the previous setting.
For example, turning off High Performance selects the Balanced plan even if a custom plan was active before.

- Display: disable MPO, hardware-accelerated GPU scheduling, optimizations and VRR for windowed games.
- System: disable Fast Startup, visual effects for best performance.
- Power: High Performance plan, PCI Express link power management off, USB selective suspend off.
- Games: Game Mode, background recording (Game DVR).
- Storage: disable hibernation (deletes `hiberfil.sys`).
- Privacy: advertising ID, tailored experiences, suggested apps, web results in Start, activity history,
  minimal telemetry.
- **Profiles**: Jogos, Economia de energia, Privacidade and Padrão do Windows apply a group of tweaks at once.
- **Boot time**: the last boots from Windows' Diagnostics-Performance log and what Windows flagged as slow.
- **Startup programs**: Run keys and Startup folders (the same way as Task Manager, `StartupApproved`),
  scheduled tasks with logon/boot triggers, and third-party automatic services (set to manual).

## History and safety

- Every change (tweaks, startup items, cleanups, deletions, moves, repairs, black screens seen) is logged in
  `%LOCALAPPDATA%\com.windowsfoldermap.app\history.json`; tweaks and startup items can be undone there.
- Optionally (on by default) a System Restore point is created before the first change of each session.
- The app checks GitHub releases once a day and shows a button when a newer version exists. Pushing a `v*`
  tag builds the installers and publishes the release.

Changes under `HKLM` need administrator rights. The app shows whether it is elevated and has a
**Reopen as administrator** button.

## Structure

```
crates/diskscan/     Scanner and tree (pure Rust, no Tauri; has tests)
crates/systools/     Diagnostics, optimizations, startup programs, restore points, history (Windows APIs behind cfg; tests run anywhere)
src-tauri/           Tauri app: commands (scan, query, delete, Explorer, tools)
src/                 React frontend (d3-hierarchy for the layouts)
src/modules/         One component per module (DiskMap, Cleanup, Diagnostics, Optimize)
src/mockApi.ts       Simulated backend: lets you develop the UI in the browser
```

## Running

Prerequisites on Windows: [Rust](https://rustup.rs), Node 20+, and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (Visual Studio Build Tools with
"Desktop development with C++"; WebView2 already ships with Windows 10/11).

```powershell
npm install
npm run tauri dev      # development
npm run tauri build    # installer in target/release/bundle/
```

To include protected folders (other users, `System Volume Information`…), run the app
**as administrator**.

UI only, in the browser with fake data: `npm run dev` and open http://localhost:1420.

Scanner from the command line (handy for measuring performance):

```powershell
cargo run --release -p diskscan --example scan -- C:\
```

Tests: `cargo test -p diskscan -p systools`.

GitHub Actions (`.github/workflows/build.yml`) builds the Windows installer on every push.
Download it from the run's artifacts.

## Notes

- Sizes are **logical** (the file size). Compressed NTFS files can take up less on disk.
- Hard links (common in `C:\Windows\WinSxS`) are counted more than once, which is why WinSxS looks bigger
  than it really is.
- "Not accounted for" in the top bar is the space used on the partition that did not show up in the scan
  (NTFS metadata, shadow copies/restore points, folders without permission).
- Sending to the Recycle Bin **does not free space** until it is emptied.
