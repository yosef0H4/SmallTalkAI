const { setChoice } = require("./settings-fixture.cjs");
// Exercise the real Tauri/WebView2 window with an isolated Settings profile.
const assert = require("node:assert/strict");
const { spawn, execFileSync } = require("node:child_process");
const { randomBytes } = require("node:crypto");
const fs = require("node:fs");
const net = require("node:net");
const os = require("node:os");
const path = require("node:path");
const { setTimeout: sleep } = require("node:timers/promises");
const { chromium } = require("playwright");

async function waitFor(name, check, timeout = 20000) {
  const deadline = Date.now() + timeout;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const result = await check();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await sleep(100);
  }
  throw Error(`Timed out waiting for ${name}`, { cause: lastError });
}

async function freePort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const { port } = server.address();
  await new Promise((resolve) => server.close(resolve));
  return port;
}

function nativeWindow(pid, action) {
  const script = `
import ctypes as c
from ctypes import wintypes as w
import sys
user = c.windll.user32
pid, action = int(sys.argv[1]), sys.argv[2]
user.IsWindowVisible.argtypes = [w.HWND]
user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
user.IsIconic.argtypes = [w.HWND]
user.IsZoomed.argtypes = [w.HWND]
user.GetWindowTextLengthW.argtypes = [w.HWND]
user.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
user.ClientToScreen.argtypes = [w.HWND, c.POINTER(w.POINT)]
user.ShowWindow.argtypes = [w.HWND, c.c_int]
user.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
callback = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
def visit(hwnd, _):
    owner = w.DWORD()
    user.GetWindowThreadProcessId(hwnd, c.byref(owner))
    if owner.value != pid or not user.IsWindowVisible(hwnd) or not user.GetWindowTextLengthW(hwnd):
        return True
    if action == "minimize":
        user.ShowWindow(hwnd, 6)
    if action == "restore":
        user.ShowWindow(hwnd, 9)
    if action == "close":
        user.PostMessageW(hwnd, 0x10, 0, 0)
    if action == "frame":
        rect, client = w.RECT(), w.POINT()
        user.GetWindowRect(hwnd, c.byref(rect))
        user.ClientToScreen(hwnd, c.byref(client))
        print("classic" if client.y - rect.top < 10 else "system")
    elif action == "zoom":
        print("maximized" if user.IsZoomed(hwnd) else "normal")
    else:
        print("minimized" if user.IsIconic(hwnd) else "visible")
    return False
user.EnumWindows(callback(visit), 0)
`;
  return execFileSync("python", ["-c", script, String(pid), action], { encoding: "utf8", timeout: 5000 }).trim();
}

function processInfo(command) {
  return execFileSync("powershell.exe", ["-NoProfile", "-Command", command], {
    encoding: "utf8", timeout: 10000, windowsHide: true,
  }).trim().split(/\s+/).filter(Boolean).map(Number);
}

function processAlive(pid) {
  try { process.kill(pid, 0); return true; }
  catch (error) { if (error.code === "ESRCH") return false; throw error; }
}

async function main() {
  assert.equal(process.platform, "win32", "This test drives Windows native window controls");
  const bin = path.resolve(process.argv[2] || "target/debug");
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), "smalltalkai-settings-window-"));
  const config = path.join(temp, "config.json");
  const port = await freePort();
  const cdp = `http://127.0.0.1:${port}`;
  const env = {
    ...process.env,
    SMALLTALKAI_CONFIG: config,
    SMALLTALKAI_RUNTIME_ROOT: path.join(temp, "runtime"),
    LOCALAPPDATA: path.join(temp, "data"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  };
  const processes = [];
  const browsers = [];
  const nativeProcesses = [];
  function launch(binary, args = []) {
    const child = spawn(path.join(bin, `${binary}.exe`), args, { env, windowsHide: true });
    child.stdout.resume();
    child.stderr.on("data", (chunk) => process.stderr.write(chunk));
    processes.push(child);
    return child;
  }
  function runCapture() {
    return launch("smalltalkai-capture", ["--token", randomBytes(32).toString("hex"), "--config", config]);
  }
  async function settingsPage() {
    await waitFor("WebView2 debug listener", async () => (await fetch(`${cdp}/json/version`)).ok);
    const browser = await chromium.connectOverCDP(cdp);
    browsers.push(browser);
    const context = browser.contexts()[0];
    const page = await waitFor("Settings document", () => context.pages().find((tab) => tab.url().includes("127.0.0.1")));
    await page.locator("#ui-language, #model-setup-heading").waitFor();
    const health = await page.evaluate(async () => {
      const token = new URLSearchParams(location.hash.slice(1)).get("token");
      const response = await fetch("/api/health", { headers: { "X-SmallTalkAI-Token": token } });
      return response.json();
    });
    assert.equal(health.ok, true);
    nativeProcesses.push(health.pid);
    await waitFor("native Settings window", () => nativeWindow(health.pid, "state") === "visible");
    return { page, context, pid: health.pid };
  }
  try {
    fs.writeFileSync(config, JSON.stringify({ general: { ui_theme: "light" } }));
    const started = Date.now();
    const launcher = launch("smalltalkai-settings");
    const { page, context, pid: host } = await settingsPage();
    const startup = Date.now() - started;
    assert.equal(launcher.exitCode, 0, "Only the Tauri Settings process should remain");
    assert.equal(nativeWindow(host, "state"), "visible", "The Settings server must own the window");
    await page.getByRole("heading", { name: "Appearance", exact: true }).waitFor();
    await page.getByRole("button", { name: "System", exact: true }).click();
    await page.locator("html[data-theme=auto]").waitFor({ state: "attached" });
    assert.equal(JSON.parse(fs.readFileSync(config, "utf8")).general.ui_theme, "auto");
    await page.getByRole("button", { name: "Light", exact: true }).click();
    await page.locator("html[data-theme=light]").waitFor({ state: "attached" });
    await page.getByRole("button", { name: "Next", exact: true }).click();
    await page.getByRole("button", { name: "Skip this feature" }).click();
    await page.getByRole("button", { name: "Skip this feature" }).click();
    await page.getByRole("button", { name: "Skip this feature" }).click();
    await page.getByRole("heading", { name: "Keyboard setup", exact: true }).waitFor();
    await page.getByRole("button", { name: "Finish", exact: true }).click();
    await page.locator('[data-section="models"]').waitFor();
    assert.equal(JSON.parse(fs.readFileSync(path.join(temp, "model-setup.json"), "utf8")).done, true);
    await page.locator('nav button[data-page="general"]').click();
    const stateUrl = "**/api/state";
    let releaseState;
    const statePending = new Promise((resolve) => { releaseState = resolve; });
    await page.route(stateUrl, async (route) => {
      await statePending;
      await route.continue();
    });
    try {
      await page.reload({ waitUntil: "domcontentloaded" });
      await page.locator(".app-shell").waitFor();
      const firstTheme = await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme);
      assert.equal(firstTheme, "light", "Settings rendered the default theme before its configuration arrived");
    } finally {
      releaseState();
      await page.unrouteAll({ behavior: "wait" });
    }
    await page.locator("#ui-language").waitFor();
    console.log("PASS: Settings paints the saved light theme before the state response");
    assert.equal(nativeWindow(host, "minimize"), "minimized");
    const duplicate = launch("smalltalkai-settings");
    await waitFor("second launch to exit", () => duplicate.exitCode !== null);
    assert.equal(duplicate.exitCode, 0);
    await waitFor("existing native window to restore", () => nativeWindow(host, "state") === "visible");
    assert.equal(context.pages().length, 1);
    console.log(`PASS: default opens one native Settings window in ${startup} ms; relaunch restores it`);

    assert.equal(await page.locator("#service").count(), 0);
    const frame = page.locator(".settings-frame");
    const closeSettings = page.getByRole("button", { name: "Close Settings" });
    assert.equal(await closeSettings.count(), 1);
    assert.equal(await frame.getByRole("button", { name: "Close Settings" }).count(), 1);
    const language = page.locator("#ui-language");
    await setChoice(language, "en");
    await page.locator("[data-page=models]").click();
    await page.getByText("Custom model locations", { exact: true }).filter({ visible: true }).first().click();
    const unload = page.getByRole("button", { name: "Unload speech model" });
    await waitFor("capture actions disabled while offline", () => unload.isDisabled());
    const firstCapture = runCapture();
    await waitFor("capture actions enabled on connection", () => unload.isEnabled());
    const webview = await context.newCDPSession(page);
    await webview.send("Page.setWebLifecycleState", { state: "frozen" });
    firstCapture.kill();
    await waitFor("capture to exit while Settings is suspended", () => firstCapture.exitCode !== null || firstCapture.signalCode !== null);
    await webview.send("Page.setWebLifecycleState", { state: "active" });
    await waitFor("capture actions disabled after disconnect", () => unload.isDisabled());
    const capture = runCapture();
    await waitFor("capture actions reenabled after restart", () => unload.isEnabled());
    await page.locator("[data-page=general]").click();
    assert.equal(await language.getAttribute("data-value"), "en", "Reconnect discarded the unsaved draft");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await waitFor("saved Settings JSON", () => JSON.parse(fs.readFileSync(config, "utf8")).general.ui_language === "en");
    await page.locator("#notice").waitFor();
    const position = await page.locator("#notice").boundingBox();
    const settingsPane = await page.locator(".app-shell > main").boundingBox();
    const height = await page.evaluate(() => innerHeight);
    assert.ok(position.y >= settingsPane.y + settingsPane.height - 1 && position.y + position.height <= height + 1, "native notice overlapped Settings or extended past the window");
    console.log("PASS: Settings connects after capture startup, reconnects after credential rotation, and saves the retained draft");

    await frame.waitFor();
    assert.equal(nativeWindow(host, "frame"), "classic");
    async function clickFrameControl(name) {
      const selector = `.settings-frame button[aria-label="${name}"]`;
      const point = await waitFor(`${name} frame control`, async () => {
        const { result } = await webview.send("Runtime.evaluate", {
          expression: `(() => { const rect = document.querySelector(${JSON.stringify(selector)})?.getBoundingClientRect();
            return rect && { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 }; })()`,
          returnByValue: true,
        });
        return result.value;
      });
      // The host handles and cancels the popup. Send a pointer click without waiting for navigation.
      await page.mouse.click(point.x, point.y);
    }
    await clickFrameControl("Maximize Settings");
    await waitFor("Classic titlebar maximizes", () => nativeWindow(host, "zoom") === "maximized");
    await clickFrameControl("Restore Settings");
    await waitFor("Classic titlebar restores", () => nativeWindow(host, "zoom") === "normal");
    await clickFrameControl("Minimize Settings");
    await waitFor("Classic titlebar minimizes", () => nativeWindow(host, "state") === "minimized");
    assert.equal(nativeWindow(host, "restore"), "visible");
    console.log("PASS: default Classic title bar maximizes, minimizes, and restores");

    const webviews = processInfo(`
      $webviews = Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'"
      $parents = @(${host})
      while ($parents) {
        $parents = @($webviews | Where-Object { $_.ParentProcessId -in $parents } | Select-Object -ExpandProperty ProcessId)
        $parents
      }
    `);
    assert.ok(webviews.length, "No WebView2 processes identified for the isolated profile");
    const origin = new URL(page.url()).origin;
    await setChoice(language, "auto");
    const browserDialogs = [];
    page.on("dialog", async dialog => {
      browserDialogs.push(dialog.type());
      await dialog.dismiss();
    });
    const grip = await frame.locator(".settings-frame-drag").boundingBox();
    await page.mouse.move(grip.x + 75, grip.y + 10);
    await page.mouse.down();
    await page.mouse.move(grip.x + 110, grip.y + 36, { steps: 5 });
    await page.mouse.up();
    assert.deepEqual(browserDialogs, [], "Dragging dirty Settings must not prompt to leave");
    await clickFrameControl("Maximize Settings");
    await waitFor("dirty Settings maximizes", () => nativeWindow(host, "zoom") === "maximized");
    await clickFrameControl("Restore Settings");
    await waitFor("dirty Settings restores", () => nativeWindow(host, "zoom") === "normal");
    assert.deepEqual(browserDialogs, [], "Frame actions must not prompt to leave");

    assert.equal(nativeWindow(host, "close"), "visible");
    const confirmation = page.getByRole("dialog", { name: "Unsaved changes" });
    await confirmation.waitFor();
    if (process.env.SMALLTALK_SETTINGS_DIALOG_SCREENSHOT) {
      await page.screenshot({ path: process.env.SMALLTALK_SETTINGS_DIALOG_SCREENSHOT });
    }
    await confirmation.getByRole("button", { name: "Keep editing" }).click();
    assert.equal(await language.getAttribute("data-value"), "auto");
    await closeSettings.click();
    await confirmation.getByRole("button", { name: "Discard and close" }).click().catch(error => {
      if (!page.isClosed()) throw error;
    });
    assert.deepEqual(browserDialogs, []);
    await waitFor("Settings and WebView2 processes to exit", () => [host, ...webviews].every((pid) => !processAlive(pid)));
    await assert.rejects(fetch(origin));
    assert.equal(capture.exitCode, null, "Closing Settings stopped the capture service");
    console.log("PASS: titlebar Close guards the draft and releases Settings and all profile WebView2 processes; capture stays running");

    launch("smalltalkai-settings");
    const { page: fresh, pid: reopened } = await settingsPage();
    assert.equal(await fresh.locator("#ui-language").getAttribute("data-value"), "en");
    assert.equal(await fresh.locator("html").getAttribute("data-theme"), "light");
    assert.equal(await fresh.getByRole("button", { name: "Close Settings" }).count(), 1);
    await setChoice(fresh.locator("#ui-language"), "auto");
    await fresh.getByRole("button", { name: "Close Settings" }).click();
    await fresh.getByRole("dialog", { name: "Unsaved changes" })
      .getByRole("button", { name: "Save and close" }).click().catch(error => {
        if (!fresh.isClosed()) throw error;
      });
    await waitFor("Save and close to persist and exit", () => !processAlive(reopened) &&
      JSON.parse(fs.readFileSync(config, "utf8")).general.ui_language === "auto");
    launch("smalltalkai-settings");
    const { page: saved, pid: savedHost } = await settingsPage();
    assert.equal(await saved.locator("#ui-language").getAttribute("data-value"), "auto");
    assert.equal(nativeWindow(savedHost, "close"), "visible");
    await waitFor("clean titlebar Close to exit Settings", () => !processAlive(savedHost));
    console.log("PASS: Save and close persists the draft, it reopens with saved Settings, and a clean native Close exits");
  } finally {
    for (const pid of nativeProcesses) {
      if (processAlive(pid)) nativeWindow(pid, "close");
    }
    for (const child of processes.reverse()) child.kill();
    for (const browser of browsers) await browser.close().catch(() => {});
    await sleep(500);
    for (const pid of nativeProcesses) {
      if (!processAlive(pid)) continue;
      try {
        execFileSync("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
          stdio: "ignore", windowsHide: true, timeout: 10000,
        });
      } catch (error) {
        if (processAlive(pid)) throw error;
      }
    }
    fs.rmSync(temp, { recursive: true, force: true, maxRetries: 4, retryDelay: 300 });
  }
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
