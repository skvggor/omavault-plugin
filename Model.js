var IMAGE_EXTENSIONS = {
  jpg: true, jpeg: true, png: true, gif: true, webp: true, avif: true, heic: true,
  svg: true, bmp: true, tif: true, tiff: true
}

var VIDEO_EXTENSIONS = {
  mp4: true, mov: true, mkv: true, webm: true, avi: true, m4v: true, mpg: true,
  mpeg: true, wmv: true
}

var DOCUMENT_EXTENSIONS = {
  pdf: true, txt: true, md: true, doc: true, docx: true, xls: true, xlsx: true,
  ppt: true, pptx: true, odt: true, ods: true, odp: true, rtf: true, csv: true,
  pages: true, numbers: true, key: true
}

var MIN_PASSPHRASE_LENGTH = 8

function defaultStatus() {
  return {
    ok: true,
    installed: false,
    initialized: false,
    unlocked: false,
    vaultPath: "",
    mountPath: "",
    usedBytes: 0,
    fileCount: 0,
    files: [],
    pendingRecovered: 0,
    holders: []
  }
}

function parseStatus(raw) {
  var text = String(raw || "").trim()
  if (text === "") return defaultStatus()
  try {
    var parsed = JSON.parse(text)
    if (!parsed || typeof parsed !== "object") return defaultStatus()
    parsed.installed = parsed.installed === true
    parsed.initialized = parsed.initialized === true
    parsed.unlocked = parsed.unlocked === true
    parsed.usedBytes = Number(parsed.usedBytes || 0)
    parsed.fileCount = Number(parsed.fileCount || 0)
    parsed.pendingRecovered = Number(parsed.pendingRecovered || 0)
    parsed.files = Array.isArray(parsed.files) ? parsed.files : []
    parsed.holders = Array.isArray(parsed.holders)
      ? parsed.holders.map(function(holder) {
          return {
            process: String(holder.process || "unknown"),
            openPaths: Array.isArray(holder.openPaths) ? holder.openPaths : []
          }
        })
      : []
    return parsed
  } catch (error) {
    var failed = defaultStatus()
    failed.ok = false
    failed.lastError = "Failed to parse vault status"
    return failed
  }
}

function friendlyError(message) {
  var value = String(message || "").replace(/\s+/g, " ").trim()
  if (/\bpassword\b|message authentication failed/i.test(value))
    return "Incorrect passphrase"
  if (/master key/i.test(value))
    return "Invalid recovery key"
  return value
}

function parseActionOutput(raw) {
  var parsed = parseStatus(raw)
  if (!parsed.ok) {
    var message = typeof parsed.error === "string" ? parsed.error.trim() : ""
    parsed.lastError = message !== "" ? friendlyError(message) : (parsed.lastError || "Helper command failed")
    return parsed
  }
  parsed.recoveryKey = String(parsed.recoveryKey || "")
  parsed.lazy = parsed.lazy === true
  parsed.recoveredCount = Number(parsed.recoveredCount || 0)
  parsed.restored = Number(parsed.restored || 0)
  parsed.discarded = Number(parsed.discarded || 0)
  parsed.passphraseChanged = parsed.passphraseChanged === true
  return parsed
}

function parseHelperVersion(raw) {
  try {
    var parsed = JSON.parse(String(raw || ""))
    return typeof parsed.version === "string" ? parsed.version : ""
  } catch (error) {
    return ""
  }
}

function passphraseProblem(passphrase, confirmation) {
  var value = String(passphrase || "")
  if (value.length < MIN_PASSPHRASE_LENGTH)
    return "Passphrase must be at least " + MIN_PASSPHRASE_LENGTH + " characters"
  if (confirmation !== undefined && value !== String(confirmation || ""))
    return "Passphrases do not match"
  return ""
}

function fileExtension(name) {
  var value = String(name || "").toLowerCase()
  var index = value.lastIndexOf(".")
  return index >= 0 ? value.substring(index + 1) : ""
}

function fileKind(name) {
  var extension = fileExtension(name)
  if (IMAGE_EXTENSIONS[extension]) return "image"
  if (VIDEO_EXTENSIONS[extension]) return "video"
  if (DOCUMENT_EXTENSIONS[extension]) return "document"
  return "misc"
}

function fileGlyph(name) {
  var kind = fileKind(name)
  if (kind === "image") return "󰋩"
  if (kind === "video") return "󰈫"
  if (kind === "document") return "󰈙"
  return "󰈔"
}

function formatBytes(bytes) {
  var value = Number(bytes || 0)
  if (!isFinite(value) || value <= 0) return "0 B"
  var units = ["B", "KB", "MB", "GB", "TB"]
  var index = 0
  while (value >= 1000 && index < units.length - 1) {
    value = value / 1000
    index++
  }
  var decimals = value >= 100 || index === 0 ? 0 : (value >= 10 ? 1 : 2)
  return value.toFixed(decimals).replace(/\.0+$/, "").replace(/(\.\d)0$/, "$1") + " " + units[index]
}

function formatCountdown(msRemaining) {
  var seconds = Math.max(0, Math.floor(Number(msRemaining || 0) / 1000))
  if (seconds < 60) return seconds + "s"
  var minutes = Math.floor(seconds / 60)
  if (minutes < 60) return minutes + "m " + (seconds % 60) + "s"
  var hours = Math.floor(minutes / 60)
  return hours + "h " + (minutes % 60) + "m " + (seconds % 60) + "s"
}

function relativeTime(timestampSec, nowMs) {
  var timestamp = Number(timestampSec || 0)
  if (!isFinite(timestamp) || timestamp <= 0) return "Unknown time"
  var now = nowMs === undefined ? Date.now() : Number(nowMs)
  var difference = Math.max(0, Math.floor((now - timestamp * 1000) / 1000))
  if (difference < 60) return "Just now"
  var minutes = Math.floor(difference / 60)
  if (minutes < 60) return minutes + "m ago"
  var hours = Math.floor(minutes / 60)
  if (hours < 24) return hours + "h ago"
  var days = Math.floor(hours / 24)
  if (days < 30) return days + "d ago"
  var months = Math.floor(days / 30)
  if (months < 12) return months + "mo ago"
  return Math.floor(days / 365) + "y ago"
}

function fileMeta(file, nowMs) {
  if (!file) return ""
  var parts = [relativeTime(file.modifiedTs, nowMs), formatBytes(file.sizeBytes)]
  return parts.join(" · ")
}

function holderSummary(holder) {
  if (!holder) return ""
  var names = (holder.openPaths || []).map(function(path) {
    return String(path || "").split("/").pop()
  })
  var count = names.length
  var summary = count === 0 ? "" : (count === 1 ? names[0] : names[0] + " + " + (count - 1) + " more")
  return holder.process + (summary !== "" ? " · " + summary : "")
}

if (typeof module !== "undefined") {
  module.exports = {
    defaultStatus: defaultStatus,
    parseStatus: parseStatus,
    parseActionOutput: parseActionOutput,
    parseHelperVersion: parseHelperVersion,
    friendlyError: friendlyError,
    passphraseProblem: passphraseProblem,
    fileExtension: fileExtension,
    fileKind: fileKind,
    fileGlyph: fileGlyph,
    formatBytes: formatBytes,
    formatCountdown: formatCountdown,
    relativeTime: relativeTime,
    fileMeta: fileMeta,
    holderSummary: holderSummary
  }
}
