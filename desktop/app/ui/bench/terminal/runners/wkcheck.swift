// The input and selection checks, in the system WKWebView with
// real AppKit events. check.mjs compiles and drives it:
//
//   wkcheck <url of the live page> <width> <height> [<screenshot dir>]
//   wkcheck <url> <width> <height> --hold
//
// With --hold it only opens the page in a key window with an Edit menu, for
// the hand checks (live.sh), and stays open until the window is closed.
//
// It opens the live page (page.ts, case=live, on `cat` in raw mode) in
// a key window, sends key, input method and mouse events through
// NSApplication as macOS would deliver them, reads what the renderer did
// from the page's __live globals, and prints one JSON object on stdout.
//
// The window takes focus for the run. The general pasteboard is saved
// before the copy and paste checks and put back after.

import AppKit
import Carbon
import WebKit

let args = CommandLine.arguments
guard args.count >= 4, let url = URL(string: args[1]), let width = Double(args[2]), let height = Double(args[3]) else {
  FileHandle.standardError.write("usage: wkcheck <url> <width> <height> [<shots dir>]\n".data(using: .utf8)!)
  exit(2)
}
let hold = args.count > 4 && args[4] == "--hold"
let shotsDir = args.count > 4 && !hold ? args[4] : nil
setvbuf(stdout, nil, _IOLBF, 0)

final class KeyWindow: NSWindow {
  override var canBecomeKey: Bool { true }
  override var canBecomeMain: Bool { true }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)

// The Edit menu, as the app has, so that Command-C and Command-V reach the
// web view as copy: and paste:.
let mainMenu = NSMenu()
let editItem = NSMenuItem()
mainMenu.addItem(NSMenuItem())
mainMenu.addItem(editItem)
let editMenu = NSMenu(title: "Edit")
editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
editItem.submenu = editMenu
app.mainMenu = mainMenu

let window = KeyWindow(
  contentRect: NSRect(x: 0, y: 0, width: width, height: height),
  styleMask: [.titled], backing: .buffered, defer: false)
window.isReleasedWhenClosed = false
window.acceptsMouseMovedEvents = true
let view = WKWebView(frame: window.contentView!.bounds, configuration: WKWebViewConfiguration())
view.autoresizingMask = [.width, .height]
if #available(macOS 13.3, *) { view.isInspectable = true }
window.contentView!.addSubview(view)
window.setFrameOrigin(NSPoint(x: 0, y: 0))

final class Nav: NSObject, WKNavigationDelegate {
  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    FileHandle.standardError.write("web content process terminated\n".data(using: .utf8)!)
    exit(3)
  }
}
let nav = Nav()
view.navigationDelegate = nav

// MARK: - helpers

@MainActor func sleep(_ ms: Int) async { try? await Task.sleep(nanoseconds: UInt64(ms) * 1_000_000) }

/// Runs `body` as an async function in the page and returns its value,
/// passed through JSON.
@MainActor func js(_ body: String) async -> Any? {
  let wrapped = "const __r = await (async () => { \(body) })(); return JSON.stringify(__r === undefined ? null : __r);"
  do {
    let r = try await view.callAsyncJavaScript(wrapped, arguments: [:], in: nil, contentWorld: .page)
    guard let s = r as? String, let d = s.data(using: .utf8) else { return nil }
    return try JSONSerialization.jsonObject(with: d, options: [.fragmentsAllowed])
  } catch {
    return ["error": String(describing: error)]
  }
}

/// A key press, made as a CGEvent so that the input method translates its
/// key code with the current layout, as it does for the keyboard.
@MainActor func key(_ code: UInt16, _ chars: String, _ plain: String, _ mods: NSEvent.ModifierFlags = []) async {
  var flags: CGEventFlags = []
  if mods.contains(.command) { flags.insert(.maskCommand) }
  if mods.contains(.option) { flags.insert(.maskAlternate) }
  if mods.contains(.control) { flags.insert(.maskControl) }
  if mods.contains(.shift) { flags.insert(.maskShift) }
  for down in [true, false] {
    let cg = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)!
    cg.flags = flags
    cg.setIntegerValueField(.eventTargetUnixProcessID, value: Int64(getpid()))
    var e = NSEvent(cgEvent: cg)!
    if e.windowNumber != window.windowNumber {
      // Give it the window, keeping the characters the layout made.
      e = NSEvent.keyEvent(
        with: e.type, location: .zero, modifierFlags: e.modifierFlags, timestamp: e.timestamp,
        windowNumber: window.windowNumber, context: nil, characters: e.characters ?? chars,
        charactersIgnoringModifiers: e.charactersIgnoringModifiers ?? plain, isARepeat: false, keyCode: code)!
    }
    app.sendEvent(e)
    await sleep(15)
  }
  await sleep(60)
}

/// A mouse event at a point in the page's CSS pixels.
@MainActor func mouse(_ type: NSEvent.EventType, _ x: Double, _ y: Double, _ mods: NSEvent.ModifierFlags = [], clicks: Int = 1) async {
  let loc = NSPoint(x: x, y: view.bounds.height - y)
  let e = NSEvent.mouseEvent(
    with: type, location: loc, modifierFlags: mods, timestamp: ProcessInfo.processInfo.systemUptime,
    windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: clicks,
    pressure: type == .leftMouseUp ? 0 : 1)!
  app.sendEvent(e)
  await sleep(20)
}

struct Grid { var x, y, w, h: Double }
@MainActor func grid() async -> Grid {
  let g = await js("return __live.grid();") as? [String: Double] ?? [:]
  return Grid(x: g["x"] ?? 0, y: g["y"] ?? 0, w: g["w"] ?? 8, h: g["h"] ?? 17)
}
/// The centre of a cell, in CSS pixels.
func cell(_ g: Grid, _ row: Int, _ col: Int) -> (Double, Double) {
  (g.x + (Double(col) + 0.5) * g.w, g.y + (Double(row) + 0.5) * g.h)
}

@MainActor func inputs() async -> [[String: String]] {
  await js("const a = __live.inputs.splice(0); return a;") as? [[String: String]] ?? []
}
@MainActor func clear() async {
  _ = await js("await __live.write('\\x1b[2J\\x1b[3J\\x1b[H'); return null;")
  _ = await inputs()
  await sleep(100)
}

func hexOf(_ s: String) -> String { s.utf8.map { String(format: "%02x", $0) }.joined(separator: " ") }

var results: [String: Any] = [:]
func record(_ name: String, _ pass: Bool?, _ detail: String) {
  results[name] = ["pass": pass as Any, "detail": detail]
}

/// The input arriving for a key, as hex, joined.
@MainActor func sent() async -> String {
  await inputs().map { $0["hex"] ?? "" }.joined(separator: " | ")
}

@MainActor func snap(_ name: String) async {
  guard let dir = shotsDir else { return }
  let config = WKSnapshotConfiguration()
  guard let image = try? await view.takeSnapshot(configuration: config),
    let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
    let png = rep.representation(using: .png, properties: [:])
  else { return }
  try? png.write(to: URL(fileURLWithPath: "\(dir)/\(name).png"))
}


// WKWebView answers the text input client messages without declaring
// NSTextInputClient in the SDK, so they are sent by selector. The caret
// rect comes from the asynchronous variant WebKit implements, or from the
// synchronous one.
func setMarked(_ text: String, _ caret: Int) {
  let sel = NSSelectorFromString("setMarkedText:selectedRange:replacementRange:")
  typealias F = @convention(c) (AnyObject, Selector, AnyObject, NSRange, NSRange) -> Void
  unsafeBitCast(view.method(for: sel), to: F.self)(view, sel, text as NSString, NSRange(location: caret, length: 0), NSRange(location: NSNotFound, length: 0))
}
func insert(_ text: String) {
  let sel = NSSelectorFromString("insertText:replacementRange:")
  typealias F = @convention(c) (AnyObject, Selector, AnyObject, NSRange) -> Void
  unsafeBitCast(view.method(for: sel), to: F.self)(view, sel, text as NSString, NSRange(location: NSNotFound, length: 0))
}
@MainActor func caretRect() async -> NSRect? {
  let asyncSel = NSSelectorFromString("firstRectForCharacterRange:completionHandler:")
  if view.responds(to: asyncSel) {
    return await withCheckedContinuation { (c: CheckedContinuation<NSRect?, Never>) in
      typealias Done = @convention(block) (NSRect, NSRange) -> Void
      typealias F = @convention(c) (AnyObject, Selector, NSRange, Done) -> Void
      let done: Done = { rect, _ in c.resume(returning: rect) }
      unsafeBitCast(view.method(for: asyncSel), to: F.self)(view, asyncSel, NSRange(location: 0, length: 0), done)
    }
  }
  let syncSel = NSSelectorFromString("firstRectForCharacterRange:actualRange:")
  if view.responds(to: syncSel) {
    typealias F = @convention(c) (AnyObject, Selector, NSRange, UnsafeMutablePointer<NSRange>?) -> NSRect
    return unsafeBitCast(view.method(for: syncSel), to: F.self)(view, syncSel, NSRange(location: 0, length: 0), nil)
  }
  return nil
}

// MARK: - the checks

@MainActor func runChecks() async {
  // Wait for the live page.
  for _ in 0..<200 {
    if (await js("return document.title;") as? String) == "ready" { break }
    await sleep(50)
  }
  window.makeKeyAndOrderFront(nil)
  app.activate(ignoringOtherApps: true)
  window.makeFirstResponder(view)
  await sleep(300)
  _ = await js("__live.focus(); return null;")
  await sleep(200)

  let source = TISCopyCurrentKeyboardLayoutInputSource().takeRetainedValue()
  let layout = Unmanaged<CFString>.fromOpaque(TISGetInputSourceProperty(source, kTISPropertyInputSourceID)).takeUnretainedValue() as String
  results["keyboardLayout"] = layout
  results["inputElement"] = await js("return __live.inputElement();")

  // Controls: a plain textarea in the same page shows whether the driver's
  // events reach the page as the keyboard's would. A renderer check that
  // fails where its control fails says nothing about the renderer.
  _ = await js("const t = document.createElement('textarea'); t.id = 'control'; t.style.cssText = 'position:fixed;left:980px;bottom:0;width:300px;height:40px'; document.body.append(t); t.focus(); return null;")
  await sleep(100)
  await key(14, "", "e", .option)
  await key(14, "e", "e")
  let controlDead = await js("return document.getElementById('control').value;") as? String ?? ""
  results["control dead key"] = controlDead
  _ = await js("const t = document.getElementById('control'); t.value = 'copied by the control'; t.select(); return null;")
  NSPasteboard.general.clearContents()
  await key(8, "c", "c", .command)
  await sleep(200)
  results["control copy"] = NSPasteboard.general.string(forType: .string) ?? ""
  // Command-C and Command-V reach a page only through the Edit menu, which
  // macOS skips when it does not let this process become active.
  let menuWorks = (results["control copy"] as? String) == "copied by the control"
  _ = await js("document.getElementById('control').remove(); __live.focus(); return null;")
  await sleep(200)

  // Plain keys, control keys and arrows.
  await clear()
  await key(0, "a", "a")
  await key(11, "b", "b")
  let plain = await sent()
  record("type letters", plain == "61 | 62" || plain == "61 62", plain)
  await key(8, "\u{3}", "c", .control)
  let ctrlC = await sent()
  record("Control-C", ctrlC == "03", ctrlC)
  await key(123, "\u{F702}", "\u{F702}", [.function, .numericPad])
  let left = await sent()
  record("Left arrow", left == "1b 5b 44" || left == "1b 4f 44", left)
  await key(51, "\u{7F}", "\u{7F}")
  let bs = await sent()
  record("Backspace", bs == "7f", bs)
  await key(36, "\r", "\r")
  let ret = await sent()
  record("Return", ret == "0d", ret)

  // A dead key: Option-E then E gives é on the ABC and US layouts.
  await key(14, "", "e", .option)
  await key(14, "e", "e")
  let dead = await sent()
  if (results["control dead key"] as? String) == "é" {
    record("dead key Option-E, E", dead == hexOf("é"), "\(dead) (want \(hexOf("é")))")
  } else {
    record("dead key Option-E, E", nil, "not checked: the driver's events do not make dead keys (control typed \(results["control dead key"] ?? ""))")
  }

  // Option as Meta: the option is off by default, so Option-B types ∫; a
  // renderer with Meta on sends ESC b.
  await key(11, "∫", "b", .option)
  let optB = await sent()
  let optNote = optB == "1b 62" ? "ESC b (Meta)" : optB == hexOf("∫") ? "∫ (no Meta)" : optB
  record("Option-B", nil, optNote)

  // The input method: marked text, the candidate window's anchor, commit.
  await clear()
  // The cursor in the middle of the screen, at row 20, column 33.
  _ = await js("await __live.write('\\x1b[21;31Habc'); return null;")
  await sleep(150)
  let g = await grid()
  setMarked("にほ", 2)
  await sleep(150)
  setMarked("にほん", 3)
  await sleep(150)
  let during = await sent()
  record("no input while composing", during.isEmpty, during.isEmpty ? "nothing sent" : during)
  let screenRect = await caretRect() ?? .zero
  results["caret rect on screen"] = NSStringFromRect(screenRect)
  results["window frame"] = NSStringFromRect(window.frame)
  let inWindow = window.convertFromScreen(screenRect)
  let inView = view.convert(inWindow, from: nil)
  let caretX = inView.origin.x
  let caretY = view.isFlipped ? inView.origin.y : view.bounds.height - inView.origin.y - inView.size.height
  let wantX = g.x + 33 * g.w
  let wantY = g.y + 20 * g.h
  let dx = abs(caretX - wantX)
  let dy = abs(caretY - wantY)
  record(
    "candidate window at the cursor", dx <= g.w * 2 && dy <= g.h * 2,
    String(format: "anchor %.0f,%.0f, cursor cell %.0f,%.0f (off by %.0f, %.0f px)", caretX, caretY, wantX, wantY, dx, dy))
  await snap("ime-composing")
  insert("日本")
  await sleep(200)
  let committed = await sent()
  record("commit Japanese", committed.replacingOccurrences(of: " | ", with: " ") == hexOf("日本"), "\(committed) (want \(hexOf("日本")))")
  // Hangul: each syllable is replaced as it grows, then committed.
  for s in ["ㅎ", "하", "한"] {
    setMarked(s, 1)
    await sleep(80)
  }
  insert("한")
  await sleep(200)
  let hangul = await sent()
  record("commit Korean", hangul.replacingOccurrences(of: " | ", with: " ") == hexOf("한"), "\(hangul) (want \(hexOf("한")))")
  // Pinyin: the marked text is Latin, which must not be typed.
  setMarked("ni hao", 6)
  await sleep(150)
  insert("你好")
  await sleep(200)
  let pinyin = await sent()
  record("commit Chinese", pinyin.replacingOccurrences(of: " | ", with: " ") == hexOf("你好"), "\(pinyin) (want \(hexOf("你好")))")

  // Links: Command-click an OSC 8 link and a plain URL.
  await clear()
  _ = await js(
    "await __live.write('\\x1b]8;;https://example.com/osc8\\x1b\\\\osc8-link\\x1b]8;;\\x1b\\\\\\r\\nsee https://example.com/plain here\\r\\n'); return null;")
  await sleep(200)
  let lg = await grid()
  var (lx, ly) = cell(lg, 0, 3)
  // Hover first: xterm.js finds a link under the pointer on move.
  await mouse(.mouseMoved, lx - lg.w, ly, .command)
  await mouse(.mouseMoved, lx, ly, .command)
  await sleep(300)
  await mouse(.leftMouseDown, lx, ly, .command)
  await mouse(.leftMouseUp, lx, ly, .command)
  await sleep(200)
  (lx, ly) = cell(lg, 1, 12)
  await mouse(.mouseMoved, lx - lg.w, ly, .command)
  await mouse(.mouseMoved, lx, ly, .command)
  await sleep(300)
  await mouse(.leftMouseDown, lx, ly, .command)
  await mouse(.leftMouseUp, lx, ly, .command)
  await sleep(300)
  let links = await js("return __live.links.splice(0);") as? [String] ?? []
  record("Command-click OSC 8 link", links.contains("https://example.com/osc8"), links.isEmpty ? "nothing" : links.joined(separator: ", "))
  record("Command-click plain URL", links.contains { $0.hasPrefix("https://example.com/plain") }, links.isEmpty ? "nothing" : links.joined(separator: ", "))
  _ = await inputs()

  // Selection: a drag on screen, then a drag past the top edge into the
  // scrollback, then copy and paste.
  await clear()
  _ = await js(
    "let s = ''; for (let i = 1; i <= 300; i++) s += `line ${String(i).padStart(3, '0')} the quick brown fox\\r\\n`; await __live.write(s); return null;")
  await sleep(300)
  let sg = await grid()
  _ = await js("window.__buttons = 0; document.addEventListener('pointermove', (e) => { window.__buttons = Math.max(window.__buttons, e.buttons); }, true); return null;")
  var (x0, y0) = cell(sg, 2, 0)
  let (x1, y1) = cell(sg, 5, 12)
  await mouse(.leftMouseDown, x0 - sg.w * 0.4, y0)
  for i in 1...6 {
    await mouse(.leftMouseDragged, x0 + (x1 - x0) * Double(i) / 6, y0 + (y1 - y0) * Double(i) / 6)
  }
  await mouse(.leftMouseUp, x1, y1)
  await sleep(200)
  let onScreen = await js("return __live.selection();") as? String
  let screenLines = onScreen?.split(separator: "\n", omittingEmptySubsequences: false).count ?? 0
  let buttons = await js("return window.__buttons;") as? Int ?? 0
  results["DOM buttons while dragging"] = buttons
  record("select by dragging", screenLines == 4 ? true : buttons == 0 ? nil : false, onScreen.map { "\(screenLines) lines: \($0.prefix(60).replacingOccurrences(of: "\n", with: "⏎"))…" } ?? "no selection")

  // Copy, with the pasteboard put back afterwards.
  let pb = NSPasteboard.general
  let saved = pb.pasteboardItems?.map { item -> NSPasteboardItem in
    let copy = NSPasteboardItem()
    for t in item.types { if let d = item.data(forType: t) { copy.setData(d, forType: t) } }
    return copy
  } ?? []
  pb.clearContents()
  await key(8, "c", "c", .command)
  await sleep(300)
  let copied = pb.string(forType: .string)
  if !menuWorks {
    record("Command-C copies the selection", nil, "not checked: Command-C did not reach the control either")
  } else if onScreen == nil {
    record("Command-C copies the selection", nil, "not checked: nothing could be selected to copy")
  } else {
    record("Command-C copies the selection", copied == onScreen, copied.map { "\($0.count) chars" } ?? "pasteboard empty")
  }
  _ = await inputs()

  // Into the scrollback: drag from row 5 above the top edge and hold.
  (x0, y0) = cell(sg, 5, 0)
  await mouse(.leftMouseDown, x0 - sg.w * 0.4, y0)
  await mouse(.leftMouseDragged, x0, sg.y + sg.h)
  for _ in 0..<40 { await mouse(.leftMouseDragged, x0, sg.y - 10); await sleep(25) }
  await mouse(.leftMouseUp, x0, sg.y - 10)
  await sleep(200)
  let back = await js("return __live.selection();") as? String
  let backLines = back?.split(separator: "\n", omittingEmptySubsequences: false).count ?? 0
  record(
    "select into scrollback by dragging past the top", backLines > 8 ? true : (onScreen == nil && buttons == 0) ? nil : false,
    back.map { "\(backLines) lines, from \($0.prefix(8))" } ?? "no selection")
  await snap("selection-scrollback")
  _ = await js("return null;")

  // Paste, with bracketed paste on as a shell would turn it on.
  _ = await js("await __live.write('\\x1b[?2004h'); return null;")
  _ = await inputs()
  pb.clearContents()
  pb.setString("pasted text", forType: .string)
  await key(9, "v", "v", .command)
  await sleep(300)
  let pastedAll = await js("return __live.inputs.splice(0);") as? [[String: String]] ?? []
  let pasted = pastedAll.map { $0["text"] ?? "" }.joined()
  if !menuWorks {
    record("Command-V pastes, bracketed", nil, "not checked: Command-C did not reach the control either")
  } else {
    record(
      "Command-V pastes, bracketed", pasted == "\u{1b}[200~pasted text\u{1b}[201~",
      pastedAll.isEmpty ? "nothing sent" : pastedAll.map { "\($0["kind"] ?? ""): \($0["hex"] ?? "")" }.joined(separator: " | "))
  }
  pb.clearContents()
  if !saved.isEmpty { pb.writeObjects(saved) }

  let data = try! JSONSerialization.data(withJSONObject: results, options: [.sortedKeys])
  print(String(data: data, encoding: .utf8)!)
  exit(0)
}

view.load(URLRequest(url: url))
window.makeKeyAndOrderFront(nil)
if hold {
  window.styleMask = [.titled, .closable, .resizable, .miniaturizable]
  window.title = "Pinrail terminal check: \(url.query ?? "")"
  window.setContentSize(NSSize(width: width, height: height))
  window.center()
  window.makeKeyAndOrderFront(nil)
  app.activate(ignoringOtherApps: true)
  window.makeFirstResponder(view)
  final class Closer: NSObject, NSWindowDelegate {
    func windowWillClose(_ notification: Notification) { exit(0) }
  }
  let closer = Closer()
  window.delegate = closer
  objc_setAssociatedObject(window, "closer", closer, .OBJC_ASSOCIATION_RETAIN)
} else {
  Task { @MainActor in await runChecks() }
  DispatchQueue.main.asyncAfter(deadline: .now() + 120) {
    FileHandle.standardError.write("timed out\n".data(using: .utf8)!)
    exit(4)
  }
}
app.run()
