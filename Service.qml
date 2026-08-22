import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "Model.js" as Model

Item {
  id: root

  property var settings: ({})

  property bool installed: false
  property bool initialized: false
  property bool helperMissing: false
  property bool unlocked: false
  property string mountPath: ""
  property string vaultPath: ""
  property double usedBytes: 0
  property int fileCount: 0
  property var files: []
  property var holders: []
  property int pendingRecovered: 0
  property string recoveryKey: ""
  property bool recoveryKeySeen: false
  property bool unlockedWithRecoveryKey: false
  property string actionStatus: ""
  property string lastError: ""

  property string _pendingPassphrase: ""
  // Kept only in memory while the vault stays unlocked via the recovery key,
  // so the user can set a new passphrase before it locks again.
  property string _recoveryKeyInUse: ""
  property string _statusOutput: ""
  property string _statusError: ""
  property string _helperProbeOutput: ""

  readonly property int recentLimit: intSetting("recentLimit", 10, 5, 50)
  readonly property int autoLockMinutes: intSetting("autoLockMinutes", 10, 1, 480)
  readonly property bool busy: statusProcess.running || initProcess.running || unlockProcess.running
    || lockProcess.running || restoreProcess.running || discardProcess.running
    || setPassphraseProcess.running
  readonly property double unlockedAtMs: unlocked ? _unlockedAtMs : 0

  // A lock was requested but open files still hold the mount (lazy detach);
  // the retry timer keeps calling lock() until the kernel releases it.
  property bool lockPending: false

  property double _unlockedAtMs: 0

  // The helper binary is installed beside this file by the install script.
  readonly property string helperPath:
    String(Qt.resolvedUrl("omavault-helper")).replace(/^file:\/\//, "")
  readonly property string setupScriptPath:
    String(Qt.resolvedUrl("setup-helper.sh")).replace(/^file:\/\//, "")
  readonly property string manifestPath:
    String(Qt.resolvedUrl("manifest.json")).replace(/^file:\/\//, "")
  property string _manifestVersion: ""

  function setting(name, fallback) {
    var value = settings ? settings[name] : undefined
    return value === undefined || value === null ? fallback : value
  }

  function intSetting(name, fallback, min, max) {
    var number = parseInt(String(setting(name, fallback)), 10)
    if (!isFinite(number)) number = fallback
    if (number < min) number = min
    if (number > max) number = max
    return number
  }

  function elideStatus(text) {
    var value = String(text || "").replace(/\s+/g, " ").trim()
    return value.length > 140 ? value.substring(0, 137) + "…" : value
  }

  function refresh() {
    if (statusProcess.running || helperPath === "") return
    _statusOutput = ""
    statusProcess.command = [helperPath, "status", "--limit", String(recentLimit)]
    statusProcess.running = true
  }

  // Spawn failures of the helper itself are not reliably reported by every
  // Quickshell build, so existence is probed through /bin/sh instead.
  function probeHelper() {
    if (helperProbe.running || helperPath === "") return
    _helperProbeOutput = ""
    helperProbe.command = [
      "sh", "-c", "test -x \"$1\" && echo yes || echo no", "omavault-helper-probe", helperPath
    ]
    helperProbe.running = true
  }

  // A stale binary (plugin updated, helper not re-downloaded) must send the
  // user back to the install button, so the reported version is compared
  // against the manifest beside this file.
  function manifestVersion() {
    if (_manifestVersion !== "") return _manifestVersion
    try {
      var parsed = JSON.parse(manifestFile.text())
      _manifestVersion = typeof parsed.version === "string" ? parsed.version : ""
    } catch (error) {
      _manifestVersion = ""
    }
    return _manifestVersion
  }

  function checkHelperVersion() {
    if (versionProbe.running) return
    versionProbe.command = [helperPath, "--version"]
    versionProbe.running = true
  }

  function applyStatus(parsed) {
    installed = parsed.installed === true
    initialized = parsed.initialized === true
    helperMissing = false
    var wasUnlocked = unlocked
    unlocked = parsed.unlocked === true
    vaultPath = String(parsed.vaultPath || "")
    mountPath = String(parsed.mountPath || "")
    usedBytes = Number(parsed.usedBytes || 0)
    fileCount = Number(parsed.fileCount || 0)
    files = parsed.files || []
    holders = parsed.holders || []
    pendingRecovered = Number(parsed.pendingRecovered || 0)
    // lastError is intentionally left untouched here: errors must stay
    // visible until the user retries, not vanish on the next status poll.
    if (unlocked && !wasUnlocked) {
      _unlockedAtMs = Date.now()
      autoLockTimer.restart()
    } else if (!unlocked) {
      autoLockTimer.stop()
      _unlockedAtMs = 0
      lockPending = false
      unlockedWithRecoveryKey = false
      _recoveryKeyInUse = ""
    } else if (lockPending && !lockRetryTimer.running && !busy) {
      lockRetryTimer.restart()
    }
  }

  function init(passphrase) {
    if (initialized || initProcess.running) return
    var problem = Model.passphraseProblem(passphrase)
    if (problem !== "") {
      lastError = problem
      return
    }
    _pendingPassphrase = passphrase
    actionStatus = "Creating vault…"
    initProcess.command = [helperPath, "init"]
    initProcess.running = true
  }

  function unlock(secret, useRecoveryKey) {
    if (!initialized || unlockProcess.running) return
    _pendingPassphrase = secret
    _recoveryKeyInUse = useRecoveryKey ? secret : ""
    actionStatus = useRecoveryKey ? "Unlocking with recovery key…" : "Unlocking…"
    unlockProcess.command = useRecoveryKey
      ? [helperPath, "unlock", "--recovery-key"]
      : [helperPath, "unlock"]
    unlockProcess.running = true
  }

  function resetPassphrase(newPassphrase) {
    if (!unlocked || !unlockedWithRecoveryKey || _recoveryKeyInUse === "" || setPassphraseProcess.running) return
    var problem = Model.passphraseProblem(newPassphrase)
    if (problem !== "") {
      lastError = problem
      return
    }
    _pendingPassphrase = _recoveryKeyInUse + "\n" + newPassphrase
    actionStatus = "Setting new passphrase…"
    setPassphraseProcess.command = [helperPath, "set-passphrase"]
    setPassphraseProcess.running = true
  }

  function renewLock() {
    if (!unlocked) return
    _unlockedAtMs = Date.now()
    autoLockTimer.restart()
    actionStatus = "Auto-lock postponed"
  }

  function lock() {
    if (!unlocked || lockProcess.running) return
    lockPending = true
    actionStatus = "Locking…"
    lockProcess.command = [helperPath, "lock"]
    lockProcess.running = true
  }

  function restoreRecovered() {
    if (!unlocked || pendingRecovered === 0 || restoreProcess.running) return
    actionStatus = "Moving recovered files into the vault…"
    restoreProcess.command = [helperPath, "restore"]
    restoreProcess.running = true
  }

  function discardRecovered() {
    if (pendingRecovered === 0 || discardProcess.running) return
    actionStatus = "Deleting recovered files…"
    discardProcess.command = [helperPath, "discard"]
    discardProcess.running = true
  }

  function openFolder() {
    if (!unlocked || mountPath === "") return
    Quickshell.execDetached(["uwsm-app", "--", "xdg-open", mountPath])
  }

  function installDependencies() {
    actionStatus = "Installing gocryptfs…"
    Quickshell.execDetached(["omarchy-launch-terminal", "omarchy-pkg-add", "gocryptfs", "fuse3"])
  }

  function installHelper() {
    actionStatus = "Installing vault helper…"
    Quickshell.execDetached(["omarchy-launch-terminal", "bash", setupScriptPath])
  }

  function openFile(file) {
    if (!file || !file.path) return
    // Reveal in the file manager instead of xdg-open: terminal-based default
    // editors (Terminal=true) launch headless from non-interactive contexts.
    Quickshell.execDetached(["uwsm-app", "--", "nautilus", "--select", fileUri(String(file.path))])
  }

  function fileUri(path) {
    var parts = String(path || "").split("/")
    for (var index = 0; index < parts.length; index++) parts[index] = encodeURIComponent(parts[index])
    return "file://" + parts.join("/")
  }

  function clearPassphrase() {
    _pendingPassphrase = ""
  }

  function handleActionExit(processName, exitCode, stdout, stderr) {
    var combined = String(stdout || "") + String(stderr || "")
    var parsed = Model.parseActionOutput(combined)
    actionStatus = ""
    if (exitCode === 0 && parsed.ok) {
      lastError = ""
      if (processName === "init") {
        recoveryKey = parsed.recoveryKey
        recoveryKeySeen = false
      } else if (processName === "lock" && parsed.lazy) {
        actionStatus = "Finishing lock — close the apps still holding vault files"
      } else if (processName === "unlock") {
        unlockedWithRecoveryKey = _recoveryKeyInUse !== ""
      }
      if (processName === "unlock" && parsed.recoveredCount > 0) {
        actionStatus = parsed.recoveredCount + " file(s) saved into the folder while locked were moved to 'recovered'"
      } else if (processName === "restore") {
        pendingRecovered = 0
        actionStatus = parsed.restored + " item(s) moved into the vault and encrypted again"
      } else if (processName === "discard") {
        pendingRecovered = 0
        actionStatus = parsed.discarded + " item(s) permanently deleted"
      } else if (processName === "set-passphrase" && parsed.passphraseChanged) {
        unlockedWithRecoveryKey = false
        _recoveryKeyInUse = ""
        actionStatus = "Passphrase updated — use it to unlock from now on"
      }
    } else {
      if (processName === "unlock") _recoveryKeyInUse = ""
      lastError = elideStatus(parsed.lastError || combined || "Helper command failed")
    }
    refresh()
  }

  component PassphraseProcess: Process {
    id: passphraseProcess

    property string actionName: ""
    stdinEnabled: true
    command: []
    stdout: StdioCollector { id: passphraseStdout; waitForEnd: true }
    stderr: StdioCollector { id: passphraseStderr; waitForEnd: true }
    onStarted: {
      write(root._pendingPassphrase + "\n")
      root.clearPassphrase()
    }
    onExited: function(exitCode) {
      root.handleActionExit(passphraseProcess.actionName, exitCode, passphraseStdout.text, passphraseStderr.text)
    }
  }

  component ActionProcess: Process {
    id: actionProcess

    property string actionName: ""
    command: []
    stdout: StdioCollector { id: actionStdout; waitForEnd: true }
    stderr: StdioCollector { id: actionStderr; waitForEnd: true }
    onExited: function(exitCode) {
      root.handleActionExit(actionProcess.actionName, exitCode, actionStdout.text, actionStderr.text)
    }
  }

  Timer {
    id: refreshTimer
    interval: 15000
    repeat: true
    running: true
    triggeredOnStart: true
    onTriggered: root.probeHelper()
  }

  Timer {
    id: autoLockTimer
    interval: root.autoLockMinutes * 60000
    repeat: false
    running: false
    onTriggered: {
      root.lock()
      statusTimer.restart()
    }
  }

  Timer {
    id: lockRetryTimer
    interval: 30000
    repeat: false
    running: false
    onTriggered: {
      if (root.unlocked && root.lockPending) root.lock()
    }
  }

  Timer {
    id: statusTimer
    interval: 800
    repeat: false
    onTriggered: root.refresh()
  }

  Timer {
    id: actionStatusTimer
    interval: 2500
    repeat: false
    onTriggered: root.actionStatus = ""
  }

  onActionStatusChanged: if (actionStatus !== "") actionStatusTimer.restart()

  Process {
    id: statusProcess
    running: false
    command: []
    stdout: StdioCollector { id: statusStdout; waitForEnd: true; onStreamFinished: root._statusOutput = text }
    stderr: StdioCollector { id: statusStderr; waitForEnd: true; onStreamFinished: root._statusError = text }
    onExited: function(exitCode) {
      var stdout = String(statusStdout.text || root._statusOutput || "")
      var stderr = String(statusStderr.text || root._statusError || "")
      if (exitCode === 0) root.applyStatus(Model.parseStatus(stdout))
      else root.lastError = root.elideStatus(stderr || stdout || "Could not read vault status")
    }
  }

  Process {
    id: helperProbe
    running: false
    command: []
    stdout: StdioCollector { id: helperProbeStdout; waitForEnd: true }
    onExited: function() {
      var answer = String(helperProbeStdout.text || root._helperProbeOutput || "").trim()
      if (answer !== "yes") {
        root.helperMissing = true
        return
      }
      root.checkHelperVersion()
    }
  }

  Process {
    id: versionProbe
    running: false
    command: []
    stdout: StdioCollector { id: versionProbeStdout; waitForEnd: true }
    onExited: function(exitCode) {
      var expected = root.manifestVersion()
      var reported = exitCode === 0 ? Model.parseHelperVersion(versionProbeStdout.text) : ""
      root.helperMissing = reported === ""
        || (expected !== "" && reported !== expected)
      if (!root.helperMissing) root.refresh()
    }
  }

  FileView {
    id: manifestFile
    path: root.manifestPath
    printErrors: false
  }

  PassphraseProcess {
    id: initProcess
    actionName: "init"
  }

  PassphraseProcess {
    id: unlockProcess
    actionName: "unlock"
  }

  PassphraseProcess {
    id: setPassphraseProcess
    actionName: "set-passphrase"
  }

  ActionProcess {
    id: lockProcess
    actionName: "lock"
  }

  ActionProcess {
    id: restoreProcess
    actionName: "restore"
  }

  ActionProcess {
    id: discardProcess
    actionName: "discard"
  }
}
