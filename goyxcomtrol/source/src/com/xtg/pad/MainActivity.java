package com.xtg.pad;

import android.app.Activity;
import android.content.Intent;
import android.content.res.Configuration;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.Settings;
import android.text.InputType;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.WindowManager;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.Toast;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import rikka.shizuku.Shizuku;

public final class MainActivity extends Activity implements Sheet.Host {

  private static final String PC_UA =
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) "
      + "Chrome/126.0.0.0 Safari/537.36";

  private FrameLayout root;
  private WebView web;
  private PadView pad;
  private Prefs prefs;
  private Mat mat;
  private Sheet sheet;
  private TextView status;
  private LinearLayout editBar;
  private Button fab;
  private String bridgeJs = "";
  private String profile = "Default";
  private boolean portrait = false;

  @Override protected void onCreate(Bundle b) {
    super.onCreate(b);
    prefs = new Prefs(this);
    mat = new Mat(this);
    profile = prefs.getStr("profile", "Default");
    portrait = getResources().getConfiguration().orientation == Configuration.ORIENTATION_PORTRAIT;

    getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
    if (Build.VERSION.SDK_INT >= 28) {
      getWindow().getAttributes().layoutInDisplayCutoutMode =
          WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
    }
    hideBars();

    PadState st = Core.state;
    st.density = getResources().getDisplayMetrics().density;
    st.sens = prefs.getInt("sens", 175);
    st.sensY = prefs.getInt("sensY", 135);
    st.stillMs = prefs.getInt("stillMs", 53);
    st.deadzone = prefs.getInt("dz", 0);
    st.mouseSens = prefs.getInt("mouseSens", 140);
    st.invertY = prefs.getBool("invY", false);
    st.kbm = prefs.getBool("kbm", false);
    st.connected = true;

    root = new FrameLayout(this);
    root.setBackgroundColor(Color.BLACK);

    web = new WebView(this);
    bridgeJs = asset("bridge.js");
    applyWebSettings();
    web.setWebViewClient(new WebViewClient() {
      @Override public void onPageStarted(WebView v, String url, android.graphics.Bitmap f) {
        v.evaluateJavascript(prelude(), null);
      }
      @Override public void onPageFinished(WebView v, String url) {
        v.evaluateJavascript(prelude() + ";try{window.__xtgConn(true);"
            + "window.__xtgKbm(" + Core.state.kbm + ")}catch(e){}", null);
        prefs.putStr("url", url);
      }
      @Override public boolean shouldOverrideUrlLoading(WebView v, WebResourceRequest r) { return false; }
    });
    web.addJavascriptInterface(new WebBridge(st), "XTGN");
    root.addView(web, new FrameLayout.LayoutParams(-1, -1));

    pad = new PadView(this, st, prefs.loadLayout(profile, portrait));
    applyPadPrefs();
    pad.onSelect = new Runnable() { @Override public void run() { buildEditBar(); } };
    root.addView(pad, new FrameLayout.LayoutParams(-1, -1));

    fab = new Button(this);
    fab.setText("\u2699");
    fab.setTextSize(TypedValue.COMPLEX_UNIT_SP, 17);
    fab.setBackground(mat.pill(Mat.alpha(mat.card, 190), dens()));
    fab.setTextColor(mat.accent);
    FrameLayout.LayoutParams flp = new FrameLayout.LayoutParams(dp(44), dp(44));
    flp.gravity = Gravity.TOP | Gravity.END;
    flp.rightMargin = dp(8); flp.topMargin = dp(8);
    fab.setOnClickListener(new View.OnClickListener() {
      @Override public void onClick(View v) { openSheet(); } });
    fab.setOnLongClickListener(new View.OnLongClickListener() {
      @Override public boolean onLongClick(View v) { togglePad(); return true; } });
    root.addView(fab, flp);

    status = new TextView(this);
    status.setTextColor(mat.accent);
    status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 10);
    status.setBackground(mat.round(Mat.alpha(mat.bg, 170), 8, dens()));
    status.setPadding(dp(8), dp(3), dp(8), dp(3));
    FrameLayout.LayoutParams slp = new FrameLayout.LayoutParams(-2, -2);
    slp.gravity = Gravity.TOP | Gravity.START;
    slp.leftMargin = dp(8); slp.topMargin = dp(8);
    root.addView(status, slp);
    status.setVisibility(View.GONE);

    editBar = new LinearLayout(this);
    editBar.setOrientation(LinearLayout.VERTICAL);
    editBar.setBackground(mat.round(mat.bg, 18, dens()));
    editBar.setVisibility(View.GONE);
    FrameLayout.LayoutParams elp = new FrameLayout.LayoutParams(-1, -2);
    elp.gravity = Gravity.BOTTOM;
    elp.setMargins(dp(8), 0, dp(8), dp(8));
    root.addView(editBar, elp);

    setContentView(root);

    Core.link = new ShizukuLink(this, new ShizukuLink.Listener() {
      @Override public void onState(final String text, final boolean live) {
        runOnUiThread(new Runnable() { @Override public void run() {
          Core.note = text;
          status.setVisibility(View.VISIBLE);
          status.setText("Shizuku: " + text);
          if (live) Core.startPump();
          if (sheet != null) sheet.build();
        } });
      }
    });
    try {
      Shizuku.addBinderReceivedListenerSticky(new Shizuku.OnBinderReceivedListener() {
        @Override public void onBinderReceived() {
          if (prefs.getBool("shizuku", false) && prefs.getBool("shizukuAuto", true))
            Core.link.connect();
        }
      });
    } catch (Throwable ignored) {}

    web.loadUrl(prefs.getStr("url", "https://www.xbox.com/play"));
    if (prefs.getBool("overlay", false)) overlay(true);
  }

  /* ---------------------------- plumbing ---------------------------- */
  private float dens() { return getResources().getDisplayMetrics().density; }
  private int dp(int v) { return (int) (v * dens()); }
  private void toast(String s) { Toast.makeText(this, s, Toast.LENGTH_SHORT).show(); }

  private String prelude() {
    return (prefs.getBool("pc", false) ? "window.__XTG_PC=true;" : "") + bridgeJs;
  }

  private void applyWebSettings() {
    WebSettings s = web.getSettings();
    s.setJavaScriptEnabled(true);
    s.setDomStorageEnabled(true);
    s.setDatabaseEnabled(true);
    s.setMediaPlaybackRequiresUserGesture(false);
    s.setSupportMultipleWindows(false);
    boolean pc = prefs.getBool("pc", false);
    if (pc) {
      // set on the WebView itself, so the request header and the JavaScript agree
      s.setUserAgentString(PC_UA);
      s.setUseWideViewPort(false);
      s.setLoadWithOverviewMode(false);
    } else {
      String ua = s.getUserAgentString();
      // a WebView marks itself with "; wv" and cloud clients use that to switch off
      if (ua != null) s.setUserAgentString(ua.replace("; wv", ""));
      s.setUseWideViewPort(true);
      s.setLoadWithOverviewMode(true);
    }
    WebView.setWebContentsDebuggingEnabled(false);
  }

  private void applyPadPrefs() {
    pad.opacity = prefs.getInt("opacity", 55);
    pad.lite = prefs.getBool("lite", false);
    pad.hud = prefs.getBool("hud", false);
    pad.floatStick = prefs.getBool("floatLs", true);
    pad.autoHideS = prefs.getInt("autoHide", 0);
    boolean an = prefs.getBool("analogT", false);
    Ctrl a = pad.byId("lt"), b2 = pad.byId("rt");
    if (a != null) a.analog = an;
    if (b2 != null) b2.analog = an;
  }

  private void hideBars() {
    getWindow().getDecorView().setSystemUiVisibility(
        View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
        | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
        | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
  }

  private String asset(String n) {
    try {
      InputStream in = getAssets().open(n);
      ByteArrayOutputStream o = new ByteArrayOutputStream();
      byte[] buf = new byte[8192]; int r;
      while ((r = in.read(buf)) > 0) o.write(buf, 0, r);
      in.close();
      return o.toString("UTF-8");
    } catch (Exception e) { return ""; }
  }

  private void openSheet() {
    if (sheet != null) return;
    sheet = new Sheet(this, this);
    root.addView(sheet, new FrameLayout.LayoutParams(-1, -1));
  }

  /* ---------------------------- edit bar ---------------------------- */
  private void buildEditBar() {
    editBar.removeAllViews();
    final Ctrl c = pad.selected;

    TextView t = new TextView(this);
    t.setText(c == null ? "Drag a control to move it. Tap one to change it."
        : "Editing: " + c.name + (c.lab != null ? "  \"" + c.lab + "\"" : ""));
    t.setTextColor(mat.text);
    t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
    t.setPadding(dp(10), dp(8), dp(10), dp(4));
    editBar.addView(t);

    LinearLayout row1 = row();
    row1.addView(eb("Size \u2212", new Runnable() { @Override public void run() { resize(0.9f); } }));
    row1.addView(eb("Size +", new Runnable() { @Override public void run() { resize(1.11f); } }));
    if (c != null && (c.kind == Ctrl.ZONE || c.kind == Ctrl.LZONE)) {
      row1.addView(eb("Taller", new Runnable() { @Override public void run() {
        c.h = Math.min(1f, c.h * 1.1f); pad.invalidate(); } }));
      row1.addView(eb("Shorter", new Runnable() { @Override public void run() {
        c.h = Math.max(0.12f, c.h * 0.9f); pad.invalidate(); } }));
    } else if (c != null) {
      row1.addView(eb("Fade \u2212", new Runnable() { @Override public void run() {
        c.op = Math.max(10, c.op - 15); pad.invalidate(); } }));
      row1.addView(eb("Fade +", new Runnable() { @Override public void run() {
        c.op = Math.min(100, c.op + 15); pad.invalidate(); } }));
    }
    editBar.addView(row1);

    LinearLayout row2 = row();
    if (c != null && c.kind == Ctrl.BTN) {
      row2.addView(eb(c.square ? "Round" : "Square", new Runnable() { @Override public void run() {
        c.square = !c.square; pad.invalidate(); buildEditBar(); } }));
      row2.addView(eb("Rename", new Runnable() { @Override public void run() { rename(c); } }));
      row2.addView(eb("Sends: " + sendName(c.btn), new Runnable() {
        @Override public void run() { remap(c); } }));
    }
    if (c != null) {
      row2.addView(eb(c.on ? "Hide" : "Show", new Runnable() { @Override public void run() {
        c.on = !c.on; pad.invalidate(); buildEditBar(); } }));
    }
    if (row2.getChildCount() > 0) editBar.addView(row2);

    LinearLayout row3 = row();
    row3.addView(eb("Reset all", new Runnable() { @Override public void run() {
      pad.setCtrls(Layouts.preset(profile, portrait)); applyPadPrefs(); buildEditBar(); } }));
    row3.addView(eb("Done", new Runnable() { @Override public void run() { endEdit(); } }));
    editBar.addView(row3);
  }

  private LinearLayout row() {
    LinearLayout l = new LinearLayout(this);
    l.setOrientation(LinearLayout.HORIZONTAL);
    return l;
  }

  private Button eb(String label, final Runnable r) {
    Button b = new Button(this);
    b.setText(label);
    b.setAllCaps(false);
    b.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
    b.setTextColor(mat.text);
    b.setBackground(mat.pill(mat.card, dens()));
    b.setPadding(dp(8), dp(4), dp(8), dp(4));
    b.setMinWidth(0); b.setMinimumWidth(0);
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0, -2, 1f);
    lp.setMargins(dp(3), dp(3), dp(3), dp(3));
    b.setLayoutParams(lp);
    b.setOnClickListener(new View.OnClickListener() {
      @Override public void onClick(View v) { r.run(); } });
    return b;
  }

  private String sendName(int btn) {
    for (int i = 0; i < Ctrl.SENDS.length; i++) if (Ctrl.SENDS[i] == btn) return Ctrl.SEND_NAMES[i];
    return "-";
  }

  private void remap(final Ctrl c) {
    int cur = 0;
    for (int i = 0; i < Ctrl.SENDS.length; i++) if (Ctrl.SENDS[i] == c.btn) cur = i;
    final int next = (cur + 1) % Ctrl.SENDS.length;
    c.btn = Ctrl.SENDS[next];
    if (c.lab == null) c.lab = Ctrl.SEND_NAMES[next];
    pad.invalidate();
    buildEditBar();
  }

  private void rename(final Ctrl c) {
    final EditText in = new EditText(this);
    in.setText(c.lab == null ? c.name : c.lab);
    in.setInputType(InputType.TYPE_CLASS_TEXT);
    new android.app.AlertDialog.Builder(this)
        .setTitle("Label for " + c.name)
        .setView(in)
        .setPositiveButton("OK", new android.content.DialogInterface.OnClickListener() {
          @Override public void onClick(android.content.DialogInterface di, int w) {
            String s = in.getText().toString().trim();
            c.lab = s.length() == 0 ? null : s;
            pad.invalidate(); buildEditBar();
          } })
        .setNeutralButton("Default", new android.content.DialogInterface.OnClickListener() {
          @Override public void onClick(android.content.DialogInterface di, int w) {
            c.lab = null; pad.invalidate(); buildEditBar(); } })
        .setNegativeButton("Cancel", null)
        .show();
  }

  private void resize(float k) {
    Ctrl c = pad.selected;
    if (c == null) { toast("Tap a control first"); return; }
    c.w = Math.max(0.05f, Math.min(1.2f, c.w * k));
    pad.invalidate();
  }

  private void endEdit() {
    pad.edit = false;
    pad.selected = null;
    editBar.setVisibility(View.GONE);
    saveProfile(profile);
    pad.invalidate();
  }

  /* ---------------------------- Sheet.Host ---------------------------- */
  @Override public PadState state() { return Core.state; }
  @Override public PadView pad() { return pad; }
  @Override public Prefs prefs() { return prefs; }
  @Override public String currentUrl() { String u = web.getUrl(); return u == null ? "" : u; }
  @Override public String profile() { return profile; }
  @Override public boolean padOn() { return pad.getVisibility() == View.VISIBLE; }
  @Override public String shizukuState() { return Core.note.isEmpty() ? "not started" : Core.note; }

  @Override public void applyUA() { applyWebSettings(); web.reload(); }
  @Override public void reload() { web.reload(); }
  @Override public void loadUrl(String u) { web.loadUrl(u); }

  @Override public void applyKbm() {
    PadState st = Core.state;
    if (!st.kbm) st.kbmReleaseAll();
    st.lx = st.ly = st.rx = st.ry = 0; st.lt = st.rt = 0;
    for (int i = 0; i < PadState.NB; i++) st.btn[i] = false;
    web.evaluateJavascript("try{window.__xtgKbm(" + st.kbm + ")}catch(e){}", null);
    pad.invalidate();
  }

  @Override public void startEdit() {
    pad.edit = true;
    pad.selected = null;
    buildEditBar();
    editBar.setVisibility(View.VISIBLE);
    pad.setVisibility(View.VISIBLE);
    pad.invalidate();
  }

  @Override public void togglePad() {
    boolean wasOn = padOn();
    pad.setVisibility(wasOn ? View.GONE : View.VISIBLE);
    Core.state.connected = !wasOn;
    if (wasOn) {
      PadState st = Core.state;
      st.lx = st.ly = st.rx = st.ry = 0; st.lt = st.rt = 0;
      for (int i = 0; i < PadState.NB; i++) st.btn[i] = false;
      st.camUp();
      if (st.kbm) st.kbmReleaseAll();
    }
    toast(wasOn ? "Pad off - the page is yours" : "Pad on");
  }

  @Override public void loadProfile(String name) {
    profile = name;
    prefs.putStr("profile", name);
    pad.setCtrls(prefs.loadLayout(name, portrait));
    applyPadPrefs();
    pad.invalidate();
    if (sheet != null) sheet.build();
  }

  @Override public void saveProfile(String name) {
    prefs.saveLayout(name, portrait, pad.ctrls());
  }

  @Override public void shizukuConnect() { Core.link.connect(); }
  @Override public void shizukuDisconnect() { Core.link.disconnect(); Core.stopPump(); }

  @Override public void overlay(boolean on) {
    Intent i = new Intent(this, OverlayService.class);
    if (!on) { stopService(i); return; }
    if (Build.VERSION.SDK_INT >= 23 && !Settings.canDrawOverlays(this)) {
      startActivity(new Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
          Uri.parse("package:" + getPackageName())));
      toast("Allow the overlay, then turn this on again");
      prefs.putBool("overlay", false);
      return;
    }
    startService(i);
  }

  @Override public void close() {
    if (sheet == null) return;
    root.removeView(sheet);
    sheet = null;
    hideBars();
  }

  /* ---------------------------- lifecycle ---------------------------- */
  @Override public void onConfigurationChanged(Configuration c) {
    super.onConfigurationChanged(c);
    boolean p = c.orientation == Configuration.ORIENTATION_PORTRAIT;
    if (p != portrait) {
      saveProfile(profile);                   // keep the layout we are leaving
      portrait = p;
      pad.setCtrls(prefs.loadLayout(profile, portrait));
      applyPadPrefs();
    }
    hideBars();
  }

  @Override public void onBackPressed() {
    if (sheet != null) { close(); return; }
    if (pad.edit) { endEdit(); return; }
    if (web.canGoBack()) { web.goBack(); return; }
    super.onBackPressed();
  }

  @Override protected void onResume() { super.onResume(); hideBars(); }

  @Override protected void onDestroy() {
    saveProfile(profile);
    super.onDestroy();
  }
}
