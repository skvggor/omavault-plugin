import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

Panel {
  id: root
  moduleName: "skvggor.omavault"
  ipcTarget: "skvggor.omavault"
  manageIpc: false

  property string focusSection: "input"
  property int fileIndex: 0
  property int inputIndex: 0
  property bool cursorActive: false
  property bool useRecoveryKey: false
  property string formError: ""

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color dim: Qt.darker(foreground, 1.55)
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property bool headerHasCursor: cursorActive && focusSection === "header" && vault.initialized
  property double nowMs: Date.now()
  readonly property double msUntilAutoLock: vault.unlocked
    ? Math.max(0, vault.unlockedAtMs + vault.autoLockMinutes * 60000 - nowMs)
    : 0
  readonly property bool lockUrgent: vault.unlocked && !vault.lockPending && msUntilAutoLock <= 60000

  function ensureCursor() {
    var stops = inputStops()
    // A form is present: keep the cursor inside it unless the user already
    // moved to another section (header, action buttons, file list).
    if (stops.length > 0) {
      if (focusSection !== "files" && focusSection !== "header" && focusSection !== "open") {
        focusSection = "input"
        if (inputIndex >= stops.length) inputIndex = stops.length - 1
        if (inputIndex < 0) inputIndex = 0
        fileIndex = 0
      }
      return
    }
    // No form: the input section is stale, fall back to the list/header.
    if (focusSection === "input") focusSection = "files"
    if (vault.files.length === 0) {
      focusSection = "header"
      fileIndex = 0
      return
    }
    if (focusSection !== "files" && focusSection !== "header" && focusSection !== "open") focusSection = "files"
    if (fileIndex >= vault.files.length) fileIndex = Math.max(0, vault.files.length - 1)
    if (fileIndex < 0) fileIndex = 0
  }

  function inputStops() {
    if (!vault.installed) return [installDependenciesButton]
    if (vault.recoveryKey !== "" && !vault.recoveryKeySeen) return [recoveryKeyCard]
    if (!vault.initialized)
      return [
        createPassphraseField, createPassphraseField.revealControl,
        confirmPassphraseField, confirmPassphraseField.revealControl,
        createVaultButton
      ]
    if (!vault.unlocked)
      return [
        unlockPassphraseField, unlockPassphraseField.revealControl,
        unlockButton, unlockModeToggle
      ]
    if (vault.unlockedWithRecoveryKey)
      return [
        newPassphraseField, newPassphraseField.revealControl,
        confirmNewPassphraseField, confirmNewPassphraseField.revealControl,
        setPassphraseButton
      ]
    return []
  }

  function clearInputCursor() {
    var stops = inputStops()
    for (var i = 0; i < stops.length; i++) {
      var stop = stops[i]
      if (stop.isPasswordField) continue
      if (stop.cursorOn !== undefined) stop.cursorOn = false
      else stop.hasCursor = false
    }
  }

  function setInputCursor(stop) {
    if (stop.isPasswordField) {
      stop.forceActiveFocus()
    } else {
      if (stop.cursorOn !== undefined) stop.cursorOn = true
      else stop.hasCursor = true
      keyCatcher.forceActiveFocus()
      scrollItemIntoView(stop)
    }
  }

  function activateInputStop() {
    var stops = inputStops()
    if (inputIndex >= stops.length) return
    var stop = stops[inputIndex]
    if (stop.isPasswordField) {
      stop.forceActiveFocus()
      return
    }
    if (stop.isRevealToggle) {
      stop.toggleReveal(false)
      return
    }
    if (stop === createVaultButton) createVault()
    else if (stop === unlockButton) unlockVault()
    else if (stop === setPassphraseButton) setPassphrase()
    else if (stop === installDependenciesButton) vault.installDependencies()
    else if (stop === recoveryKeyCard) acknowledgeRecoveryKey()
    else if (stop === unlockModeToggle) toggleUnlockMode()
  }

  // Returns true when the cursor ran past the first or last element of the
  // panel, so the caller (Tab navigation) can move on to the next plugin.
  function moveCursor(dx, dy) {
    cursorActive = true
    ensureCursor()
    if (dy === 0) return false
    // Linear order mirrors the panel layout: header, action buttons, form
    // (when the vault was unlocked with the recovery key), file list.
    if (focusSection === "input") {
      var stops = inputStops()
      if (stops.length === 0) return dy > 0
      var next = inputIndex + (dy > 0 ? 1 : -1)
      if (next < 0) {
        if (vault.unlocked && !vault.lockPending) setOpenCursor()
        else setHeaderCursor()
        return false
      }
      if (next >= stops.length) {
        if (vault.files.length > 0) {
          focusSection = "files"
          fileIndex = 0
          scrollCursorIntoView()
          return false
        }
        return true
      }
      clearInputCursor()
      inputIndex = next
      setInputCursor(stops[inputIndex])
      return false
    }
    if (focusSection === "header") {
      if (dy > 0) {
        if (vault.unlocked && !vault.lockPending) setOpenCursor()
        else if (inputStops().length > 0) {
          clearInputCursor()
          focusSection = "input"
          inputIndex = 0
          setInputCursor(inputStops()[0])
        } else if (vault.files.length > 0) {
          focusSection = "files"
          fileIndex = 0
          scrollCursorIntoView()
        }
      }
      return dy < 0
    }
    if (focusSection === "open") {
      if (dy < 0) setHeaderCursor()
      else if (inputStops().length > 0) {
        clearInputCursor()
        focusSection = "input"
        inputIndex = 0
        setInputCursor(inputStops()[0])
      } else if (vault.files.length > 0) {
        focusSection = "files"
        fileIndex = 0
        scrollCursorIntoView()
      } else return true
      return false
    }
    if (focusSection === "files") {
      if (dy < 0 && fileIndex === 0) {
        var above = inputStops()
        if (above.length > 0) {
          clearInputCursor()
          focusSection = "input"
          inputIndex = above.length - 1
          setInputCursor(above[inputIndex])
        } else if (vault.unlocked) setOpenCursor()
        else setHeaderCursor()
        return false
      }
      if (dy > 0 && fileIndex >= vault.files.length - 1) return true
      fileIndex = Math.max(0, Math.min(vault.files.length - 1, fileIndex + dy))
      scrollCursorIntoView()
    }
    return false
  }

  function tabNavigate(forward) {
    if (moveCursor(0, forward ? 1 : -1)) switchPanel(forward ? 1 : -1)
  }

  function setHeaderCursor() {
    cursorActive = true
    focusSection = "header"
    if (panelFlick) panelFlick.contentY = 0
  }

  function setOpenCursor() {
    cursorActive = true
    focusSection = "open"
    scrollItemIntoView(openFolderButton)
  }

  function setFileCursor(index) {
    cursorActive = true
    focusSection = "files"
    fileIndex = index
    scrollCursorIntoView()
  }

  function activateCursor() {
    ensureCursor()
    if (focusSection === "input") {
      activateInputStop()
      return
    }
    if (focusSection === "header" && vault.unlocked) vault.lock()
    else if (focusSection === "open") { vault.openFolder(); root.close() }
    else if (focusSection === "files") { vault.openFile(selectedFile()); root.close() }
  }

  function selectedFile() {
    if (vault.files.length === 0) return null
    return vault.files[Math.max(0, Math.min(fileIndex, vault.files.length - 1))]
  }

  function scrollItemIntoView(item, view) {
    var flick = view || panelFlick
    if (!flick || !item) return
    Qt.callLater(function() {
      if (!item || !flick.contentItem) return
      var margin = Style.space(6)
      var point = item.mapToItem(flick.contentItem, 0, 0)
      var top = point.y
      var bottom = top + item.height
      var viewTop = flick.contentY
      var viewBottom = viewTop + flick.height
      var maxY = Math.max(0, flick.contentHeight - flick.height)
      if (top < viewTop + margin) flick.contentY = Math.max(0, top - margin)
      else if (bottom > viewBottom - margin) flick.contentY = Math.min(maxY, bottom + margin - flick.height)
    })
  }

  function scrollCursorIntoView() {
    if (focusSection === "files" && fileColumn && fileIndex >= 0 && fileIndex < fileColumn.children.length) {
      scrollItemIntoView(fileColumn.children[fileIndex], filesFlick)
    }
  }

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onOpenedChanged: if (opened) {
    cursorActive = false
    inputIndex = 0
    useRecoveryKey = false
    formError = ""
    if (panelFlick) panelFlick.contentY = 0
    vault.refresh()
    var recoveryPending = vault.recoveryKey !== "" && !vault.recoveryKeySeen
    var field = !recoveryPending ? activePasswordField() : null
    Qt.callLater(function() {
      if (field) field.forceActiveFocus()
      else keyCatcher.forceActiveFocus()
    })
  }
  onFileIndexChanged: scrollCursorIntoView()

  Service {
    id: vault
    settings: root.settings
  }

  Connections {
    target: vault
    function onInitializedChanged() { root.ensureCursor() }
    function onUnlockedChanged() {
      root.nowMs = Date.now()
      root.useRecoveryKey = false
      root.ensureCursor()
      if (root.opened && vault.unlocked) Qt.callLater(function() { keyCatcher.forceActiveFocus() })
    }
    function onFilesChanged() { root.ensureCursor() }
  }

  IpcHandler {
    target: root.ipcTarget

    // No ": void" return annotation here: qmllint (Qt 6.11) crashes on it.
    function open() { root.open() }
    function close() { root.close() }
    function show() { root.open() }
    function hide() { root.close() }
    function toggle() { root.toggle() }
    function refresh(): string { vault.refresh(); return "ok" }
    function lock(): string { vault.lock(); return "ok" }
    function status(): string { return vault.unlocked ? "unlocked" : "locked" }
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    tooltipText: vault.helperMissing ? "Omavault — vault helper is not installed"
      : !vault.installed ? "Omavault — gocryptfs is not installed"
      : !vault.initialized ? "Omavault — no vault created yet"
      : vault.lockPending ? "Omavault — finishing lock"
      : vault.unlocked ? "Omavault — unlocked"
      : "Omavault — locked"
    iconComponent: Component {
      Item {
        VaultIcon {
          anchors.centerIn: parent
          iconSize: Style.space(12)
          color: !vault.unlocked
            ? Qt.darker(root.barForeground, 1.55)
            : (root.lockUrgent ? (root.bar ? root.bar.urgent : root.urgent) : root.barForeground)
        }
      }
    }
    onPressed: function(buttonCode) {
      if (buttonCode === Qt.RightButton) vault.refresh()
      else if (buttonCode === Qt.MiddleButton && vault.unlocked) vault.lock()
      else root.toggle()
    }
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(380))
    contentHeight: panel.fittedContentHeight(column.implicitHeight, Style.space(560))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onMoveRequested: function(dx, dy) {
        if (!root.cursorActive) { root.cursorActive = true; return }
        root.moveCursor(dx, dy)
      }
      onActivateRequested: {
        root.cursorActive = true
        root.activateCursor()
      }
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.tabNavigate(direction > 0) }
      onTextKey: function(text) {
        if (text === "r" || text === "R") vault.refresh()
        else if ((text === "l" || text === "L") && vault.unlocked) vault.lock()
        else if (root.focusSection === "input") {
          var field = vault.unlocked ? null : activePasswordField()
          if (field) field.forceActiveFocus()
        }
      }

      Flickable {
        id: panelFlick
        anchors.fill: parent
        contentWidth: width
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        interactive: contentHeight > height
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
          id: column
          width: panelFlick.width
          spacing: Style.space(12)

          Item {
            id: header
            width: parent.width
            implicitHeight: hero.implicitHeight
            readonly property bool ringVisible: root.headerHasCursor
            function focusHero() { root.setHeaderCursor() }

            VaultHero {
              id: hero
              width: parent.width
              title: "Omavault"
              meta: vault.helperMissing ? "vault helper is not installed"
                : !vault.installed ? "gocryptfs is not installed"
                : !vault.initialized ? "No vault created yet"
                : vault.unlocked
                  ? (vault.lockPending
                    ? "Finishing lock — close apps using the vault"
                    : "Auto-locks in " + Model.formatCountdown(root.msUntilAutoLock) + (root.lockUrgent ? " — postpone?" : ""))
                  : "Locked"
              metaColor: !vault.unlocked
                ? hero.dim
                : (root.lockUrgent && !vault.lockPending ? root.urgent : Color.accent)
              foreground: root.lockUrgent ? root.urgent : root.foreground
              fontFamily: root.fontFamily
              iconOpacity: vault.unlocked ? 1.0 : 0.6
              iconComponent: Component {
                VaultIcon {
                  iconSize: Style.font.display
                  color: vault.unlocked
                    ? (root.lockUrgent ? root.urgent : root.foreground)
                    : root.dim
                }
              }
              trailingControl: Component {
                ToggleSwitch {
                  id: lockSwitch
                  visible: vault.unlocked
                  checked: vault.unlocked
                  busy: vault.busy
                  hasCursor: header.ringVisible
                  foreground: hero.foreground
                  onHovered: function(on) { if (on) header.focusHero() }
                  onToggled: {
                    if (vault.unlocked) vault.lock()
                  }

                  PanelToolTip {
                    visible: lockSwitch.containsMouse
                    text: "Lock vault"
                    fontFamily: hero.fontFamily
                  }
                }
              }
            }
          }

          Text {
            visible: vault.actionStatus !== "" || vault.lastError !== ""
            width: parent.width
            text: vault.actionStatus !== "" ? vault.actionStatus : vault.lastError
            textFormat: Text.PlainText
            color: vault.lastError !== "" && vault.actionStatus === "" ? root.urgent : root.dim
            font.family: root.fontFamily
            font.pixelSize: Style.font.bodySmall
            wrapMode: Text.WordWrap
          }

          RecoveryKeyCard {
            id: recoveryKeyCard
            visible: vault.recoveryKey !== "" && !vault.recoveryKeySeen
            width: parent.width
          }

          Column {
            visible: vault.helperMissing
            width: parent.width
            spacing: Style.space(8)

            Text {
              width: parent.width
              text: "The vault helper binary is missing — this happens when the plugin is installed with omarchy plugin add instead of ./install.sh. Download the prebuilt build matching this version?"
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              wrapMode: Text.WordWrap
            }

            Button {
              id: installHelperButton
              width: parent.width
              text: "Install helper"
              iconText: "󰇚"
              foreground: root.foreground
              fontFamily: root.fontFamily
              onClicked: vault.installHelper()
            }
          }

          Column {
            visible: vault.helperStale
            width: parent.width
            spacing: Style.space(8)

            Text {
              width: parent.width
              text: vault.reportedHelperVersion === ""
                ? "A new vault helper (v" + vault.expectedHelperVersion + ") is available. Update it to stay in sync with the plugin?"
                : "The vault helper (v" + vault.reportedHelperVersion + ") is older than the plugin (v" + vault.expectedHelperVersion + "). Update it?"
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              wrapMode: Text.WordWrap
            }

            Button {
              id: updateHelperButton
              width: parent.width
              text: "Update helper"
              iconText: "󰇚"
              foreground: root.foreground
              fontFamily: root.fontFamily
              onClicked: vault.installHelper()
            }
          }

          Column {
            visible: !vault.helperMissing && !vault.installed
            width: parent.width
            spacing: Style.space(8)

            Text {
              width: parent.width
              text: "gocryptfs was not found. Install it to create and unlock the vault — your terminal will ask for your sudo password."
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              wrapMode: Text.WordWrap
            }

            Button {
              id: installDependenciesButton
              width: parent.width
              text: "Install gocryptfs"
              iconText: "󰇚"
              foreground: root.foreground
              fontFamily: root.fontFamily
              onClicked: vault.installDependencies()
            }
          }

          Column {
            visible: vault.installed && !vault.initialized && (vault.recoveryKey === "" || vault.recoveryKeySeen)
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: "CREATE VAULT"
              foreground: root.foreground
              fontFamily: root.fontFamily
            }

            PasswordField {
              id: createPassphraseField
              placeholderText: "Passphrase (min. 8 characters)"
              onTextChanged: root.formError = ""
            }

            PasswordField {
              id: confirmPassphraseField
              placeholderText: "Confirm passphrase"
              onTextChanged: root.formError = ""
            }

            Text {
              visible: root.formError !== ""
              width: parent.width
              text: root.formError
              color: root.urgent
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              wrapMode: Text.WordWrap
            }

            Button {
              id: createVaultButton
              Accessible.role: Accessible.Button
              Accessible.name: "Create vault"
              width: parent.width
              text: "Create vault"
              iconText: "󰌾"
              foreground: root.foreground
              fontFamily: root.fontFamily
              enabled: !vault.busy
              onClicked: createVault()
            }
          }

          Column {
            visible: vault.initialized && !vault.unlocked
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: "UNLOCK"
              foreground: root.foreground
              fontFamily: root.fontFamily
            }

            PasswordField {
              id: unlockPassphraseField
              placeholderText: root.useRecoveryKey
                ? "Recovery key"
                : "Passphrase"
            }

            Button {
              id: unlockButton
              Accessible.role: Accessible.Button
              Accessible.name: "Unlock vault"
              width: parent.width
              text: "Unlock"
              iconText: "󰌿"
              foreground: root.foreground
              fontFamily: root.fontFamily
              enabled: !vault.busy
              onClicked: unlockVault()
            }

            Text {
              id: unlockModeToggle
              property bool cursorOn: false

              readonly property string alternateMode: root.useRecoveryKey
                ? "Use passphrase instead"
                : "Use recovery key"

              width: parent.width
              text: alternateMode
              color: cursorOn ? root.foreground : root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              horizontalAlignment: Text.AlignHCenter
              opacity: unlockModeArea.containsMouse ? 1.0 : 0.8

              MouseArea {
                id: unlockModeArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: toggleUnlockMode()
              }
            }
          }

          Column {
            visible: vault.unlocked
            width: parent.width
            spacing: Style.space(8)

            RowLayout {
              width: parent.width
              spacing: Style.space(8)

              Button {
                Layout.fillWidth: true
                Accessible.role: Accessible.Button
                Accessible.name: "Postpone auto-lock"
                text: "Postpone lock"
                iconText: "󰚎"
                foreground: root.lockUrgent ? root.urgent : Color.accent
                accent: root.lockUrgent ? root.urgent : Color.accent
                fontFamily: root.fontFamily
                enabled: !vault.lockPending
                onClicked: vault.renewLock()
              }

              Button {
                id: openFolderButton
                Accessible.role: Accessible.Button
                Accessible.name: "Open vault folder"
                Layout.fillWidth: true
                text: "Open folder"
                iconText: "󰉋"
                foreground: root.foreground
                fontFamily: root.fontFamily
                hasCursor: root.cursorActive && root.focusSection === "open"
                onHovered: function(isHovered) { if (isHovered) root.setOpenCursor() }
                onClicked: { vault.openFolder(); root.close() }
              }
            }
          }

          Column {
            visible: vault.unlocked && vault.unlockedWithRecoveryKey
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: "NEW PASSPHRASE"
              foreground: root.urgent
              fontFamily: root.fontFamily
            }

            Text {
              width: parent.width
              text: "You unlocked with the recovery key. Set a new passphrase to unlock normally again — the recovery key stays valid."
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              wrapMode: Text.WordWrap
            }

            PasswordField {
              id: newPassphraseField
              placeholderText: "New passphrase (min. 8 characters)"
              onTextChanged: root.formError = ""
            }

            PasswordField {
              id: confirmNewPassphraseField
              placeholderText: "Confirm new passphrase"
              onTextChanged: root.formError = ""
            }

            Text {
              visible: root.formError !== ""
              width: parent.width
              text: root.formError
              color: root.urgent
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              wrapMode: Text.WordWrap
            }

            Button {
              id: setPassphraseButton
              Accessible.role: Accessible.Button
              Accessible.name: "Set new passphrase"
              width: parent.width
              text: "Set passphrase"
              iconText: "󰌇"
              foreground: Color.accent
              accent: Color.accent
              fontFamily: root.fontFamily
              enabled: !vault.busy
              onClicked: setPassphrase()
            }
          }

          Column {
            visible: vault.unlocked && vault.holders.length > 0
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: "OPEN IN APPS"
              foreground: root.urgent
              fontFamily: root.fontFamily
            }

            Text {
              width: parent.width
              text: "These apps hold files inside the vault and delay the final lock. Close them to finish locking."
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              wrapMode: Text.WordWrap
            }

            Repeater {
              model: vault.holders
              HolderRow {
                required property var modelData
                width: parent.width
                holder: modelData
              }
            }
          }

          Column {
            visible: vault.unlocked && vault.pendingRecovered > 0
            width: parent.width
            spacing: Style.space(8)

            PanelSectionHeader {
              text: "RECOVERED FILES"
              foreground: root.urgent
              fontFamily: root.fontFamily
            }

            Text {
              width: parent.width
              text: vault.pendingRecovered + " item(s) dropped into the vault while it was locked are sitting unencrypted in 'recovered'. Move them back in to encrypt them, or delete them."
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.caption
              wrapMode: Text.WordWrap
            }

            RowLayout {
              width: parent.width
              spacing: Style.space(8)

              Button {
                Layout.fillWidth: true
                Accessible.role: Accessible.Button
                Accessible.name: "Move recovered files into the vault"
                text: "Move to vault"
                iconText: "󰁯"
                foreground: Color.accent
                accent: Color.accent
                fontFamily: root.fontFamily
                enabled: !vault.busy
                onClicked: vault.restoreRecovered()
              }

              Button {
                Layout.fillWidth: true
                Accessible.role: Accessible.Button
                Accessible.name: "Delete recovered files permanently"
                text: "Delete"
                iconText: "󰆴"
                foreground: root.urgent
                accent: root.urgent
                fontFamily: root.fontFamily
                enabled: !vault.busy
                onClicked: vault.discardRecovered()
              }
            }
          }

          PanelSeparator {
            visible: vault.unlocked
            foreground: root.foreground
          }

          Column {
            visible: vault.unlocked
            width: parent.width
            spacing: Style.space(10)

            Row {
              width: parent.width

              PanelSectionHeader {
                id: recentFilesHeader
                text: "RECENT FILES"
                foreground: root.foreground
                fontFamily: root.fontFamily
              }

              Item {
                width: parent.width - recentFilesHeader.implicitWidth - filesMeta.implicitWidth
                height: 1
              }

              Text {
                id: filesMeta
                text: Model.formatBytes(vault.usedBytes) + (vault.fileCount > 0 ? " · " + vault.fileCount + " files" : "")
                color: root.dim
                font.family: root.fontFamily
                font.pixelSize: Style.font.caption
                anchors.verticalCenter: parent.verticalCenter
              }
            }

            Text {
              visible: vault.files.length === 0
              width: parent.width
              text: "No files in the vault yet."
              color: root.dim
              font.family: root.fontFamily
              font.pixelSize: Style.font.body
              horizontalAlignment: Text.AlignHCenter
            }

            Flickable {
              id: filesFlick
              visible: vault.files.length > 0
              width: parent.width
              height: Math.min(contentHeight, Style.space(220))
              contentWidth: width
              contentHeight: fileColumn.implicitHeight
              clip: true
              boundsBehavior: Flickable.StopAtBounds
              flickableDirection: Flickable.VerticalFlick
              interactive: contentHeight > height
              ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

              Column {
                id: fileColumn
                width: filesFlick.width
                spacing: Style.space(6)

                Repeater {
                  model: vault.files
                  FileRow {
                    required property var modelData
                    required property int index
                    width: fileColumn.width
                    file: modelData
                    rowIndex: index
                  }
                }
              }
            }
          }
        }
      }
    }
  }

  Timer {
    id: countdownTimer
    interval: 1000
    repeat: true
    running: root.opened && vault.unlocked
    onTriggered: root.nowMs = Date.now()
  }

  function acknowledgeRecoveryKey() {
    vault.recoveryKeySeen = true
    vault.recoveryKey = ""
    root.ensureCursor()
  }

  function activePasswordField() {
    if (!vault.initialized && vault.recoveryKey === "") return createPassphraseField
    if (!vault.unlocked) return unlockPassphraseField
    return null
  }

  function createVault() {
    var problem = Model.passphraseProblem(
      createPassphraseField.text,
      confirmPassphraseField.text
    )
    if (problem !== "") {
      formError = problem
      return
    }
    formError = ""
    vault.lastError = ""
    vault.init(createPassphraseField.text)
    createPassphraseField.text = ""
    confirmPassphraseField.text = ""
  }

  function toggleUnlockMode() {
    root.useRecoveryKey = !root.useRecoveryKey
    unlockPassphraseField.text = ""
    unlockPassphraseField.forceActiveFocus()
  }

  function unlockVault() {
    vault.lastError = ""
    vault.unlock(unlockPassphraseField.text, root.useRecoveryKey)
    unlockPassphraseField.text = ""
  }

  function setPassphrase() {
    var problem = Model.passphraseProblem(
      newPassphraseField.text,
      confirmNewPassphraseField.text
    )
    if (problem !== "") {
      formError = problem
      return
    }
    formError = ""
    vault.lastError = ""
    vault.resetPassphrase(newPassphraseField.text)
    newPassphraseField.text = ""
    confirmNewPassphraseField.text = ""
  }

  component PasswordField: TextField {
    id: field
    property bool isPasswordField: true
    property bool revealPassword: false
    readonly property alias revealControl: revealGlyph
    width: parent.width
    password: !revealPassword
    rightPadding: horizontalPadding + Border.right(_borderSpec) + Style.space(16)
    foreground: root.foreground
    font.family: root.fontFamily
    font.pixelSize: Style.font.body
    hasCursor: !activeFocus && root.cursorActive && root.focusSection === "input"
    onActiveFocusChanged: if (activeFocus) {
      root.cursorActive = true
      root.focusSection = "input"
      root.clearInputCursor()
      var stops = root.inputStops()
      for (var i = 0; i < stops.length; i++) {
        if (stops[i] === field) root.inputIndex = i
      }
    }
    onAccepted: {
      if (!vault.initialized) {
        if (field === createPassphraseField && confirmPassphraseField.text === "") {
          confirmPassphraseField.forceActiveFocus()
          return
        }
        createVault()
      } else if (vault.unlocked && vault.unlockedWithRecoveryKey) {
        if (field === newPassphraseField && confirmNewPassphraseField.text === "") {
          confirmNewPassphraseField.forceActiveFocus()
          return
        }
        setPassphrase()
      } else if (!vault.unlocked) unlockVault()
    }
    Keys.onPressed: function(event) {
      if (event.key === Qt.Key_Escape) {
        root.close()
        event.accepted = true
        return
      }
      if (event.key === Qt.Key_Down || (event.key === Qt.Key_Tab && !(event.modifiers & Qt.ShiftModifier))) {
        root.moveCursor(0, 1)
        event.accepted = true
      } else if (event.key === Qt.Key_Up || (event.key === Qt.Key_Tab && (event.modifiers & Qt.ShiftModifier))) {
        root.moveCursor(0, -1)
        event.accepted = true
      }
    }

    Item {
      id: revealGlyph
      property bool isRevealToggle: true
      property bool cursorOn: false
      function toggleReveal(refocus) {
        field.revealPassword = !field.revealPassword
        if (refocus !== false) field.forceActiveFocus()
      }

      Accessible.role: Accessible.CheckBox
      Accessible.name: "Show passphrase"
      Accessible.checked: field.revealPassword
      Accessible.onPressAction: revealGlyph.toggleReveal(false)

      anchors.right: parent.right
      anchors.rightMargin: Style.space(8)
      anchors.verticalCenter: parent.verticalCenter
      width: revealText.implicitWidth + Style.space(12)
      height: revealText.implicitHeight + Style.space(6)

      BorderSurface {
        anchors.fill: parent
        visible: revealGlyph.cursorOn || revealArea.containsMouse
        color: Style.controlFill(true, true, root.foreground, Color.accent)
        borderSpec: Border.controlSpec(
          revealGlyph.cursorOn ? "focus" : "hover-cursor",
          root.foreground, Color.accent)
        radius: Style.cornerRadius
      }

      Text {
        id: revealText
        anchors.centerIn: parent
        text: field.revealPassword ? "\u2212" : "+"
        color: root.foreground
        opacity: field.activeFocus || field.hovered || revealArea.containsMouse || revealGlyph.cursorOn ? 1.0 : 0.55
        font.bold: revealGlyph.cursorOn
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
      }

      MouseArea {
        id: revealArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: revealGlyph.toggleReveal(true)
      }
    }
  }

  component RecoveryKeyCard: CursorSurface {
    property bool cursorOn: false
    hasCursor: cursorOn
    foreground: root.foreground

    Accessible.role: Accessible.Button
    Accessible.name: "I have saved the recovery key"
    Accessible.description: "Hides the recovery key. It will not be shown again."
    Accessible.onPressAction: root.acknowledgeRecoveryKey()

    implicitHeight: recoveryKeyColumn.implicitHeight + Style.spacing.rowPaddingX

    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onEntered: recoveryKeyCard.cursorOn = true
      onExited: recoveryKeyCard.cursorOn = false
      onClicked: root.acknowledgeRecoveryKey()
    }

    Column {
      id: recoveryKeyColumn
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.space(10)
      anchors.rightMargin: Style.space(10)
      spacing: Style.space(6)

      Text {
        width: parent.width
        text: "Recovery key — save it now. It is the only way to unlock the vault if you forget the passphrase. It will not be shown again."
        color: root.urgent
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
      }

      Text {
        id: recoveryKeyText
        width: parent.width
        text: vault.recoveryKey
        textFormat: Text.PlainText
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.bodySmall
        font.bold: true
        wrapMode: Text.WordWrap
      }

      RowLayout {
        width: parent.width

        Button {
          id: copyRecoveryKeyButton
          property bool copied: false

          function copyRecoveryKey() {
            // wl-copy reads stdin until EOF, so the key never appears in
            // argv or a shell command line visible through /proc.
            recoveryKeyCopyProcess.stdinEnabled = true
            recoveryKeyCopyProcess.payload = vault.recoveryKey
            recoveryKeyCopyProcess.command = ["wl-copy"]
            recoveryKeyCopyProcess.running = true
            copyRecoveryKeyButton.copied = true
            copiedTimer.restart()
          }

          Accessible.role: Accessible.Button
          Accessible.name: "Copy recovery key to clipboard"
          text: copyRecoveryKeyButton.copied ? "Copied" : "Copy"
          iconText: copyRecoveryKeyButton.copied ? "󰄬" : "󰆏"
          foreground: Color.accent
          accent: Color.accent
          fontFamily: root.fontFamily
          onClicked: copyRecoveryKey()

          Timer {
            id: copiedTimer
            interval: 2000
            onTriggered: copyRecoveryKeyButton.copied = false
          }

          Process {
            id: recoveryKeyCopyProcess
            property string payload: ""
            stdinEnabled: true
            command: []
            stdout: StdioCollector {}
            stderr: StdioCollector {}
            onStarted: {
              write(recoveryKeyCopyProcess.payload)
              recoveryKeyCopyProcess.payload = ""
              stdinEnabled = false
            }
          }
        }

        Item {
          Layout.fillWidth: true
          height: 1
        }
      }

      Text {
        width: parent.width
        text: "Click here once you have saved it"
        color: root.dim
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }

  component FileRow: CursorSurface {
    id: fileRow
    property var file: null
    property int rowIndex: 0
    readonly property string fileName: file ? String(file.name || "Untitled") : "Untitled"

    Accessible.role: Accessible.Button
    Accessible.name: fileRow.fileName + ", " + Model.fileMeta(fileRow.file)
    Accessible.description: "Open this vault file"
    Accessible.onPressAction: { vault.openFile(fileRow.file); root.close() }

    hasCursor: root.cursorActive && root.focusSection === "files" && root.fileIndex === rowIndex
    foreground: root.foreground

    implicitHeight: fileRowContent.implicitHeight + Style.spacing.rowPaddingX

    MouseArea {
      anchors.fill: parent
      hoverEnabled: true
      cursorShape: Qt.PointingHandCursor
      onEntered: root.setFileCursor(fileRow.rowIndex)
      onClicked: { vault.openFile(fileRow.file); root.close() }
    }

    RowLayout {
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.space(10)
      anchors.rightMargin: Style.space(10)
      spacing: Style.space(8)

      Text {
        text: Model.fileGlyph(fileRow.fileName)
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.icon
        Layout.alignment: Qt.AlignVCenter
      }

      ColumnLayout {
        id: fileRowContent
        Layout.fillWidth: true
        spacing: Style.space(1)

        Text {
          Layout.fillWidth: true
          text: fileRow.fileName
          textFormat: Text.PlainText
          color: root.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          elide: Text.ElideRight
        }

        Text {
          Layout.fillWidth: true
          text: Model.fileMeta(fileRow.file)
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
        }
      }
    }
  }

  component HolderRow: CursorSurface {
    id: holderRow
    property var holder: null

    Accessible.role: Accessible.ListItem
    Accessible.name: "App holding vault files: " + Model.holderSummary(holderRow.holder)

    foreground: root.foreground
    implicitHeight: holderRowContent.implicitHeight + Style.spacing.rowPaddingX

    RowLayout {
      id: holderRowContent
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.verticalCenter: parent.verticalCenter
      anchors.leftMargin: Style.space(10)
      anchors.rightMargin: Style.space(10)
      spacing: Style.space(8)

      Text {
        text: "󰀦"
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.icon
        Layout.alignment: Qt.AlignVCenter
      }

      Text {
        Layout.fillWidth: true
        text: Model.holderSummary(holderRow.holder)
        textFormat: Text.PlainText
        color: root.foreground
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        elide: Text.ElideRight
      }
    }
  }
}
