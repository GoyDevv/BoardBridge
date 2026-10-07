package com.xtg.pad;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.graphics.Color;
import android.graphics.Typeface;
import android.text.InputType;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.SeekBar;
import android.widget.TextView;
import android.widget.Toast;

/* The settings panel. Built in code rather than XML so the whole thing can take its
   colours from the Material You palette the system derived from the wallpaper. */
public final class Sheet extends FrameLayout {

  public interface Host {
    PadState state();
    PadView pad();
    Prefs prefs();
    void applyUA();
    void reload();
    void loadUrl(String url);
    String currentUrl();
    void startEdit();
    void togglePad();
    boolean padOn();
    void loadProfile(String name);
    void saveProfile(String name);
    String profile();
    void shizukuConnect();
    void shizukuDisconnect();
    String shizukuState();
    void overlay(boolean on);
    void applyKbm();
    void close();
  }

  private final Host host;
  private final Mat m;
  private final float d;
  private final LinearLayout content = new LinearLayout(getContext());
  private final LinearLayout tabs = new LinearLayout(getContext());
  private String tab = "cam";

  private static final String[][] TABS = {
    { "cam", "Camera" }, { "pad", "Pad" }, { "lay", "Layout" },
    { "cloud", "Cloud" }, { "ctl", "Controller" }, { "about", "About" }
  };

  public Sheet(Context c, Host host) {
    super(c);
    this.host = host;
    this.m = new Mat(c);
    this.d = c.getResources().getDisplayMetrics().density;
    setBackgroundColor(Mat.alpha(m.bg, 246));

    LinearLayout root = new LinearLayout(c);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(p(16), p(14), p(16), p(10));

    LinearLayout head = new LinearLayout(c);
    head.setOrientation(LinearLayout.HORIZONTAL);
    head.setGravity(Gravity.CENTER_VERTICAL);
    TextView t = new TextView(c);
    t.setText("XTG Cloud Pad");
    t.setTextColor(m.text);
    t.setTypeface(Typeface.DEFAULT_BOLD);
    t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 19);
    head.addView(t, new LinearLayout.LayoutParams(0, -2, 1f));
    Button close = pillBtn("Close", true);
    close.setOnClickListener(new OnClickListener() {
      @Override public void onClick(View v) { host.close(); }
    });
    head.addView(close);
    root.addView(head);

    TextView sub = new TextView(c);
    sub.setText(m.dynamic ? "Material You \u2022 colours from your wallpaper" : "Material You palette");
    sub.setTextColor(m.textDim);
    sub.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11);
    sub.setPadding(0, p(2), 0, p(10));
    root.addView(sub);

    tabs.setOrientation(LinearLayout.HORIZONTAL);
    HorizontalScrollView hs = new HorizontalScrollView(c);
    hs.setHorizontalScrollBarEnabled(false);
    hs.addView(tabs);
    root.addView(hs);
    buildTabs();

    content.setOrientation(LinearLayout.VERTICAL);
    ScrollView sv = new ScrollView(c);
    sv.addView(content, new ViewGroup.LayoutParams(-1, -2));
    root.addView(sv, new LinearLayout.LayoutParams(-1, 0, 1f));

    addView(root, new FrameLayout.LayoutParams(-1, -1));
    build();
  }

  private int p(int dp) { return (int) (dp * d); }

  /* ---------------------------- widgets ---------------------------- */
  private void buildTabs() {
    tabs.removeAllViews();
    for (final String[] tb : TABS) {
      Button b = pillBtn(tb[1], tab.equals(tb[0]));
      b.setOnClickListener(new OnClickListener() {
        @Override public void onClick(View v) { tab = tb[0]; buildTabs(); build(); }
      });
      LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-2, -2);
      lp.rightMargin = p(6);
      b.setLayoutParams(lp);
      tabs.addView(b);
    }
  }

  private Button pillBtn(String label, boolean active) {
    Button b = new Button(getContext());
    b.setText(label);
    b.setAllCaps(false);
    b.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
    b.setTextColor(active ? m.onAccent : m.text);
    b.setBackground(m.pill(active ? m.accent : m.card, d));
    b.setPadding(p(16), p(6), p(16), p(6));
    b.setMinWidth(0); b.setMinimumWidth(0);
    b.setElevation(0);
    return b;
  }

  private LinearLayout card() {
    LinearLayout l = new LinearLayout(getContext());
    l.setOrientation(LinearLayout.VERTICAL);
    l.setBackground(m.round(m.card, 20, d));
    l.setPadding(p(14), p(12), p(14), p(12));
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-1, -2);
    lp.topMargin = p(10);
    l.setLayoutParams(lp);
    content.addView(l);
    return l;
  }

  private void title(LinearLayout into, String s) {
    TextView t = new TextView(getContext());
    t.setText(s); t.setTextColor(m.accent);
    t.setTypeface(Typeface.DEFAULT_BOLD);
    t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
    t.setPadding(0, 0, 0, p(4));
    into.addView(t);
  }

  private void note(LinearLayout into, String s) {
    TextView t = new TextView(getContext());
    t.setText(s); t.setTextColor(m.textDim);
    t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11);
    t.setPadding(0, p(2), 0, p(6));
    t.setLineSpacing(p(2), 1f);
    into.addView(t);
  }

  private interface IntCb { void on(int v); }
  private interface BoolCb { void on(boolean v); }
  private interface StrCb { void on(String v); }

  private void slider(LinearLayout into, final String label, final int min, int max, int val,
                      final String unit, final IntCb cb) {
    final TextView t = new TextView(getContext());
    t.setTextColor(m.text);
    t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
    t.setText(label + "   " + val + unit);
    SeekBar sb = new SeekBar(getContext());
    sb.setMax(max - min);
    sb.setProgress(Math.max(0, Math.min(max - min, val - min)));
    sb.getProgressDrawable().setTint(m.accent);
    sb.getThumb().setTint(m.accent);
    sb.setOnSeekBarChangeListener(new SeekBar.OnSeekBarChangeListener() {
      @Override public void onProgressChanged(SeekBar s, int prog, boolean u) {
        int v = prog + min;
        t.setText(label + "   " + v + unit);
        cb.on(v);
      }
      @Override public void onStartTrackingTouch(SeekBar s) {}
      @Override public void onStopTrackingTouch(SeekBar s) {}
    });
    into.addView(t);
    into.addView(sb);
  }

  private void toggle(LinearLayout into, String label, boolean on, final BoolCb cb) {
    final android.widget.Switch s = new android.widget.Switch(getContext());
    s.setText(label);
    s.setChecked(on);
    s.setTextColor(m.text);
    s.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
    s.setPadding(0, p(6), 0, p(6));
    s.setThumbTintList(android.content.res.ColorStateList.valueOf(m.accent));
    s.setOnCheckedChangeListener(new android.widget.CompoundButton.OnCheckedChangeListener() {
      @Override public void onCheckedChanged(android.widget.CompoundButton b, boolean v) { cb.on(v); }
    });
    into.addView(s);
  }

  private void seg(LinearLayout into, String label, String[][] opts, String cur, final StrCb cb) {
    if (label != null) {
      TextView t = new TextView(getContext());
      t.setText(label); t.setTextColor(m.text);
      t.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
      t.setPadding(0, p(4), 0, p(4));
      into.addView(t);
    }
    LinearLayout row = new LinearLayout(getContext());
    row.setOrientation(LinearLayout.HORIZONTAL);
    for (final String[] o : opts) {
      Button b = pillBtn(o[1], o[0].equals(cur));
      LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0, -2, 1f);
      lp.rightMargin = p(6);
      b.setLayoutParams(lp);
      b.setOnClickListener(new OnClickListener() {
        @Override public void onClick(View v) { cb.on(o[0]); build(); }
      });
      row.addView(b);
    }
    row.setPadding(0, p(2), 0, p(4));
    into.addView(row);
  }

  private void action(LinearLayout into, String label, final Runnable r) {
    Button b = pillBtn(label, false);
    b.setBackground(m.pill(m.accentDim, d));
    LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-1, -2);
    lp.topMargin = p(6);
    b.setLayoutParams(lp);
    b.setOnClickListener(new OnClickListener() {
      @Override public void onClick(View v) { r.run(); }
    });
    into.addView(b);
  }

  private void toast(String s) { Toast.makeText(getContext(), s, Toast.LENGTH_SHORT).show(); }

  /* ---------------------------- the tabs ---------------------------- */
  public void build() {
    content.removeAllViews();
    final PadState st = host.state();
    final PadView pv = host.pad();
    final Prefs pr = host.prefs();

    if (tab.equals("cam")) {
      LinearLayout c = card();
      title(c, "Input method");
      seg(c, null, new String[][]{ {"pad","Xbox pad"}, {"kbm","Mouse + keys"} },
          st.kbm ? "kbm" : "pad", new StrCb() { @Override public void on(String v) {
            st.kbm = v.equals("kbm"); pr.putBool("kbm", st.kbm); host.applyKbm(); } });
      note(c, st.kbm
        ? "The camera is a mouse, not a stick. The pixels your thumb moves become the same "
          + "number of mouse pixels, so there is no maximum deflection, no cap on turn speed and "
          + "no rate for the game to integrate. The left stick becomes WASD and the buttons send "
          + "keys. Only works in titles that accept mouse and keyboard on cloud - turn on PC mode "
          + "in the Cloud tab first, and reload."
        : "The virtual Xbox pad. Works in every cloud game, but the camera is a stick, so the "
          + "game decides your maximum turn rate once you reach full deflection.");

      LinearLayout s = card();
      title(s, "Aim");
      slider(s, "Sensitivity", 10, 400, st.sens, "%", new IntCb() {
        @Override public void on(int v) { st.sens = v; pr.putInt("sens", v); } });
      slider(s, "Vertical", 20, 200, st.sensY, "%", new IntCb() {
        @Override public void on(int v) { st.sensY = v; pr.putInt("sensY", v); } });
      if (st.kbm) {
        slider(s, "Mouse sensitivity", 10, 600, st.mouseSens, "%", new IntCb() {
          @Override public void on(int v) { st.mouseSens = v; pr.putInt("mouseSens", v); } });
        note(s, "100% is one mouse pixel per screen pixel of thumb travel.");
      }
      slider(s, "Stop threshold", 10, 80, st.stillMs, " ms", new IntCb() {
        @Override public void on(int v) { st.stillMs = v; pr.putInt("stillMs", v); } });
      note(s, "The longest the camera may keep moving on your last known thumb speed. Lower "
        + "stops sooner; too low stutters when a frame arrives late. 53 ms is the sweet spot.");
      toggle(s, "Invert vertical", st.invertY, new BoolCb() {
        @Override public void on(boolean v) { st.invertY = v; pr.putBool("invY", v); } });
      toggle(s, "Debug HUD", pv.hud, new BoolCb() {
        @Override public void on(boolean v) { pv.hud = v; pr.putBool("hud", v); pv.invalidate(); } });
      note(s, "At 400% sensitivity the stick saturates at roughly 0.4 px/ms of thumb movement. "
        + "Past full deflection the game's own turn rate is the ceiling, so if it still feels slow, "
        + "raise the in-game sensitivity instead of this slider.");
    }

    if (tab.equals("pad")) {
      LinearLayout c = card();
      title(c, "Left stick");
      seg(c, null, new String[][]{ {"float","Anywhere in its area"}, {"fixed","Fixed in place"} },
          pv.floatStick ? "float" : "fixed", new StrCb() { @Override public void on(String v) {
            pv.floatStick = v.equals("float"); pr.putBool("floatLs", pv.floatStick); pv.invalidate(); } });
      note(c, pv.floatStick
        ? "The stick appears wherever your thumb lands inside the \"Left stick area\" rectangle, "
          + "so you never have to find it. Move and resize that area in the Layout tab."
        : "The stick stays exactly where you put it.");
      slider(c, "Deadzone", 0, 40, st.deadzone, "%", new IntCb() {
        @Override public void on(int v) { st.deadzone = v; pr.putInt("dz", v); } });

      LinearLayout l = card();
      title(l, "Look");
      slider(l, "Opacity", 10, 100, pv.opacity, "%", new IntCb() {
        @Override public void on(int v) { pv.opacity = v; pr.putInt("opacity", v); pv.invalidate(); } });
      slider(l, "Fade when idle", 0, 20, pv.autoHideS, "s", new IntCb() {
        @Override public void on(int v) { pv.autoHideS = v; pr.putInt("autoHide", v); pv.invalidate(); } });
      note(l, "0 keeps the pad visible always. Anything else fades it out after that long "
        + "without a touch, and the first touch brings it straight back.");
      toggle(l, "Lite graphics (outlines only)", pv.lite, new BoolCb() {
        @Override public void on(boolean v) { pv.lite = v; pr.putBool("lite", v); pv.invalidate(); } });

      LinearLayout t = card();
      title(t, "Triggers");
      Ctrl lt = pv.byId("lt");
      toggle(t, "Analog triggers (drag up for partial pull)", lt != null && lt.analog,
          new BoolCb() { @Override public void on(boolean v) {
            Ctrl a = pv.byId("lt"), b = pv.byId("rt");
            if (a != null) a.analog = v;
            if (b != null) b.analog = v;
            pr.putBool("analogT", v); pv.invalidate(); } });
      note(t, "A real trigger is not a switch. With this on, LT and RT report how far up the "
        + "button your thumb has slid, which is what walk-versus-sprint and gentle braking need. "
        + "Shizuku mode reports it to the game as a true analog axis; browser mode reports it as "
        + "the analog value of buttons 6 and 7.");

      LinearLayout o = card();
      title(o, "Other");
      toggle(o, "Pad visible", host.padOn(), new BoolCb() {
        @Override public void on(boolean v) { host.togglePad(); } });
      note(o, "The camera zone covers half the screen, so the page behind it is unreachable "
        + "while the pad is live. Long press the gear for the same thing without opening this.");
      toggle(o, "Vibrate on press", pr.getBool("haptic", false), new BoolCb() {
        @Override public void on(boolean v) { pr.putBool("haptic", v); } });
      note(o, "Off by default, and it stays off unless you ask - vibration is latency you can feel.");
    }

    if (tab.equals("lay")) {
      LinearLayout c = card();
      title(c, "Profiles");
      seg(c, null, new String[][]{ {"Default","Default"}, {"Warzone","Warzone"},
                                   {"Saved A","A"}, {"Saved B","B"} },
          host.profile(), new StrCb() { @Override public void on(String v) {
            host.loadProfile(v); toast("Loaded " + v); } });
      note(c, "Warzone is the layout carried over from the browser extension: shoot under your "
        + "left thumb, scope on the right, sprint and slide where the thumb already is. Portrait "
        + "and landscape are stored separately, so rotating keeps both of your setups.");
      action(c, "Save the current layout into " + host.profile(), new Runnable() {
        @Override public void run() { host.saveProfile(host.profile()); toast("Saved"); } });

      LinearLayout e = card();
      title(e, "Edit");
      action(e, "Move and resize controls", new Runnable() {
        @Override public void run() { host.startEdit(); host.close(); } });
      note(e, "Drag a control to move it. Tap one and the bar at the bottom resizes it, changes "
        + "its shape, renames it, remaps what it sends, or hides it.");
      action(e, "Reset this orientation to the default", new Runnable() {
        @Override public void run() { host.loadProfile("Default"); toast("Reset"); } });

      LinearLayout v = card();
      title(v, "Show or hide each control");
      for (final Ctrl ct : pv.ctrls()) {
        toggle(v, ct.name + (ct.lab != null ? "  (\"" + ct.lab + "\")" : ""), ct.on,
            new BoolCb() { @Override public void on(boolean on) {
              ct.on = on; pv.invalidate(); host.saveProfile(host.profile()); } });
      }

      LinearLayout x = card();
      title(x, "Backup");
      action(x, "Copy this layout to the clipboard", new Runnable() { @Override public void run() {
        ClipboardManager cm = (ClipboardManager) getContext().getSystemService(Context.CLIPBOARD_SERVICE);
        cm.setPrimaryClip(ClipData.newPlainText("xtg", pr.layoutJson(host.profile(), pv.ctrls())));
        toast("Copied"); } });
      final EditText in = new EditText(getContext());
      in.setHint("paste a layout here");
      in.setTextColor(m.text);
      in.setHintTextColor(m.textDim);
      in.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_MULTI_LINE);
      in.setBackground(m.round(m.cardHi, 12, d));
      in.setPadding(p(10), p(8), p(10), p(8));
      x.addView(in);
      action(x, "Load the pasted layout", new Runnable() { @Override public void run() {
        if (pr.applyLayoutJson(in.getText().toString(), pv.ctrls())) {
          pv.invalidate(); host.saveProfile(host.profile()); toast("Loaded");
        } else toast("That did not look like a layout");
      } });
    }

    if (tab.equals("cloud")) {
      LinearLayout c = card();
      title(c, "Pretend to be a PC");
      toggle(c, "PC mode", pr.getBool("pc", false), new BoolCb() {
        @Override public void on(boolean v) { pr.putBool("pc", v); host.applyUA(); } });
      note(c, "This is the thing a browser extension cannot do properly. The app sets the user "
        + "agent on the WebView itself, so the HTTP request and the JavaScript tell the same "
        + "story - desktop Chrome on Windows, with no touchscreen, a fine hovering pointer and no "
        + "ontouchstart. An extension can only lie in JavaScript while the request header still "
        + "says Android, and the cloud client notices and refuses to start. Turning this on "
        + "reloads the page, because the client decides once, at start-up, what kind of device "
        + "you are.");
      action(c, "Reload now", new Runnable() { @Override public void run() { host.reload(); } });

      LinearLayout u = card();
      title(u, "Go to");
      final EditText url = new EditText(getContext());
      url.setText(host.currentUrl());
      url.setTextColor(m.text);
      url.setSingleLine(true);
      url.setInputType(InputType.TYPE_TEXT_VARIATION_URI);
      url.setBackground(m.round(m.cardHi, 12, d));
      url.setPadding(p(10), p(8), p(10), p(8));
      u.addView(url);
      action(u, "Open", new Runnable() { @Override public void run() {
        String s = url.getText().toString().trim();
        if (s.length() == 0) return;
        if (!s.startsWith("http")) s = "https://" + s;
        host.loadUrl(s); host.close(); } });
      action(u, "Xbox Cloud Gaming", new Runnable() { @Override public void run() {
        host.loadUrl("https://www.xbox.com/play"); host.close(); } });
      action(u, "GeForce NOW", new Runnable() { @Override public void run() {
        host.loadUrl("https://play.geforcenow.com"); host.close(); } });
      action(u, "Gamepad tester", new Runnable() { @Override public void run() {
        host.loadUrl("https://hardwaretester.com/gamepad"); host.close(); } });
      note(u, "The page the app opens next time is whichever one you were last on.");
    }

    if (tab.equals("ctl")) {
      LinearLayout c = card();
      title(c, "Real controller through Shizuku");
      toggle(c, "Create a kernel-level Xbox controller", pr.getBool("shizuku", false),
          new BoolCb() { @Override public void on(boolean v) {
            pr.putBool("shizuku", v);
            if (v) host.shizukuConnect(); else host.shizukuDisconnect();
            build(); } });
      TextView s = new TextView(getContext());
      s.setText(host.shizukuState());
      s.setTextColor(m.accent);
      s.setTextSize(TypedValue.COMPLEX_UNIT_SP, 11);
      s.setPadding(0, p(4), 0, p(4));
      c.addView(s);
      action(c, "Retry", new Runnable() { @Override public void run() {
        host.shizukuConnect(); build(); } });
      note(c, "Not an API spoof: a real HID device, created in the kernel through /dev/uhid by a "
        + "helper running with shell rights. Every app on the phone sees it - this browser, "
        + "Quetta, Chrome, the official Xbox app - as a Microsoft 045E:02EA Xbox Wireless "
        + "Controller, with analog triggers.\n\n"
        + "Why shell is allowed: sepolicy grants \"allow shell uhid_device:chr_file "
        + "rw_file_perms\", /dev/uhid is mode 0660 owner uhid group uhid, and adbd puts AID_UHID "
        + "in the shell process's groups on purpose. And /dev/uhid needs no ioctl at all - you "
        + "write one packed struct to create the device and one per report - which is why this "
        + "works in pure Java with no native code.\n\n"
        + "Shizuku must be installed and started. On Android 11 and newer wireless debugging is "
        + "enough, with no PC, but it has to be restarted after a reboot unless you are rooted.");
      toggle(c, "Connect automatically on launch", pr.getBool("shizukuAuto", true),
          new BoolCb() { @Override public void on(boolean v) { pr.putBool("shizukuAuto", v); } });

      LinearLayout o = card();
      title(o, "Use it over other apps");
      toggle(o, "Show the pad on top of everything", pr.getBool("overlay", false),
          new BoolCb() { @Override public void on(boolean v) {
            pr.putBool("overlay", v); host.overlay(v); } });
      note(o, "A small round button sits in the top-right corner of every app; tap it to put the "
        + "pad up or take it down. It is two windows on purpose - a full-screen overlay swallows "
        + "every touch inside its bounds and Android gives you no way to hand an unclaimed one "
        + "back to the app underneath, so only the button stays on screen permanently.");
    }

    if (tab.equals("about")) {
      LinearLayout c = card();
      title(c, "Two ways in");
      note(c, "Browser mode needs no setup. The page's own getGamepads() call reaches the touch "
        + "state synchronously through a Java method, in the same JavaScript task - no event hop, "
        + "no frame boundary, nothing queued. Touch is read natively, every historical sample in "
        + "a batched MotionEvent with its own timestamp. Measured travel ratio is 1.000: the "
        + "camera delivers exactly as much turn as your thumb asked for. The browser extension "
        + "sits at 1.07 to 1.15 because a content script cannot see the digitiser this directly.\n\n"
        + "Shizuku mode creates a real controller and works everywhere on the phone, with analog "
        + "triggers, at up to 250 reports a second and only when a byte actually changed.");
      LinearLayout v = card();
      title(v, "Version");
      note(v, "XTG Cloud Pad 1.4.0. No vibration anywhere unless you switch it on. No analytics, "
        + "no network access of its own - the only thing it loads is the page you ask for.");
      note(v, "A virtual HID device is visible to everything on the phone while it exists, "
        + "including anything that enumerates input devices. Cloud gaming streams your input to a "
        + "server and does not care, but it is worth knowing.");
    }
  }
}
