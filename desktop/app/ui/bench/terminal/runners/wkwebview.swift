// Loads a bench page in the system WKWebView, in a real window, as the app
// does on macOS. run.mjs compiles and drives it:
//
//   wkwebview-runner <url> <width> <height>
//
// stdout: "pid <n>" with the web content process id, for memory samples;
//         "snapped <path>" after a screenshot.
// stdin:  "snap <path>" saves a PNG of the #main element.
//
// The window stays on screen at the bottom left without taking focus.
// Occlusion detection is turned off so that animation frames keep running
// when other windows cover it.

import AppKit
import WebKit

let args = CommandLine.arguments
guard args.count >= 4, let url = URL(string: args[1]), let width = Double(args[2]), let height = Double(args[3]) else {
  FileHandle.standardError.write("usage: wkwebview-runner <url> <width> <height>\n".data(using: .utf8)!)
  exit(2)
}
setvbuf(stdout, nil, _IOLBF, 0)

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let window = NSWindow(
  contentRect: NSRect(x: 0, y: 0, width: width, height: height),
  styleMask: [.borderless], backing: .buffered, defer: false)
window.isReleasedWhenClosed = false
let config = WKWebViewConfiguration()
let view = WKWebView(frame: window.contentView!.bounds, configuration: config)
view.autoresizingMask = [.width, .height]
if #available(macOS 13.3, *) { view.isInspectable = true }
// Private, but stable for years: keep the page "visible" when covered.
let occlusionSelector = NSSelectorFromString("_setWindowOcclusionDetectionEnabled:")
if view.responds(to: occlusionSelector) {
  typealias SetBool = @convention(c) (AnyObject, Selector, Bool) -> Void
  unsafeBitCast(view.method(for: occlusionSelector), to: SetBool.self)(view, occlusionSelector, false)
} else {
  FileHandle.standardError.write("occlusion detection cannot be turned off\n".data(using: .utf8)!)
}
window.contentView!.addSubview(view)
window.setFrameOrigin(NSPoint(x: 0, y: 0))
window.orderFrontRegardless()

final class Nav: NSObject, WKNavigationDelegate {
  func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
    if let pid = webView.value(forKey: "_webProcessIdentifier") as? NSNumber {
      print("pid \(pid)")
    }
  }
  func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
    FileHandle.standardError.write("web content process terminated\n".data(using: .utf8)!)
    exit(3)
  }
}
let nav = Nav()
view.navigationDelegate = nav
view.load(URLRequest(url: url))

@MainActor func snapshot(to path: String) {
  let js = "(() => { const b = document.querySelector('#main').getBoundingClientRect(); return [b.x, b.y, b.width, b.height]; })()"
  view.evaluateJavaScript(js) { result, _ in
    guard let r = result as? [NSNumber], r.count == 4 else { print("snapped \(path)"); return }
    let config = WKSnapshotConfiguration()
    config.rect = CGRect(x: r[0].doubleValue, y: r[1].doubleValue, width: r[2].doubleValue, height: r[3].doubleValue)
    view.takeSnapshot(with: config) { image, _ in
      if let image = image, let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
        let png = rep.representation(using: .png, properties: [:])
      {
        try? png.write(to: URL(fileURLWithPath: path))
      }
      print("snapped \(path)")
    }
  }
}

Thread.detachNewThread {
  while let line = readLine() {
    let parts = line.split(separator: " ", maxSplits: 1).map(String.init)
    if parts.count == 2 && parts[0] == "snap" {
      DispatchQueue.main.async { snapshot(to: parts[1]) }
    }
  }
  // stdin closed: the driver is gone.
  exit(0)
}

app.run()
