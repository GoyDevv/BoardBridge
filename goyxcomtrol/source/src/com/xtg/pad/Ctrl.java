package com.xtg.pad;

import org.json.JSONObject;

/* One on-screen control. The centre is a fraction of the screen and the size a fraction
   of the shorter edge, so a layout survives rotation, a different phone and a notch. */
public final class Ctrl {
  public static final int ZONE = 0, STICK = 1, DPAD = 2, BTN = 3, LZONE = 4;

  public final String id;
  public final int kind;
  public final String name;
  public int btn;                // PadState index, remappable
  public String lab;             // custom label, null = use the default
  public float x, y, w, h;
  public int op = 100;           // own opacity, % of the global one
  public boolean square = false;
  public boolean on = true;
  public boolean analog = false; // triggers: drag up for partial travel

  public Ctrl(String id, int kind, int btn, String name, float x, float y, float w, float h) {
    this.id = id; this.kind = kind; this.btn = btn; this.name = name;
    this.x = x; this.y = y; this.w = w; this.h = h;
  }

  public String label() {
    if (lab != null) return lab;
    return name;
  }

  public JSONObject toJson() {
    JSONObject o = new JSONObject();
    try {
      o.put("id", id); o.put("x", x); o.put("y", y); o.put("w", w); o.put("h", h);
      o.put("op", op); o.put("sq", square); o.put("on", on); o.put("btn", btn);
      o.put("an", analog);
      if (lab != null) o.put("lab", lab);
    } catch (Exception ignored) {}
    return o;
  }

  public void fromJson(JSONObject o) {
    x = (float) o.optDouble("x", x); y = (float) o.optDouble("y", y);
    w = (float) o.optDouble("w", w); h = (float) o.optDouble("h", h);
    op = o.optInt("op", op); square = o.optBoolean("sq", square);
    on = o.optBoolean("on", on); btn = o.optInt("btn", btn);
    analog = o.optBoolean("an", analog);
    String l = o.optString("lab", null);
    lab = (l == null || l.length() == 0 || "null".equals(l)) ? null : l;
  }

  public Ctrl copy() {
    Ctrl c = new Ctrl(id, kind, btn, name, x, y, w, h);
    c.op = op; c.square = square; c.on = on; c.lab = lab; c.analog = analog;
    return c;
  }

  /* ---- the layout that ships ---- */
  public static Ctrl[] defaults(boolean portrait) {
    Ctrl[] cs = portrait ? port() : land();
    return cs;
  }

  private static Ctrl[] land() {
    return new Ctrl[] {
      new Ctrl("rs",   ZONE,  -1,             "Camera zone",      0.70f, 0.50f, 0.60f, 1.00f),
      new Ctrl("lsz",  LZONE, -1,             "Left stick area",  0.19f, 0.70f, 0.38f, 0.61f),
      new Ctrl("ls",   STICK, -1,             "Left stick",       0.14f, 0.70f, 0.30f, 0.30f),
      new Ctrl("dpad", DPAD,  -1,             "D-pad",            0.07f, 0.30f, 0.26f, 0.26f),
      new Ctrl("a",    BTN,   PadState.A,     "A",                0.88f, 0.76f, 0.14f, 0.14f),
      new Ctrl("b",    BTN,   PadState.B,     "B",                0.96f, 0.60f, 0.14f, 0.14f),
      new Ctrl("x",    BTN,   PadState.X,     "X",                0.80f, 0.60f, 0.14f, 0.14f),
      new Ctrl("y",    BTN,   PadState.Y,     "Y",                0.88f, 0.44f, 0.14f, 0.14f),
      new Ctrl("lb",   BTN,   PadState.LB,    "LB",               0.19f, 0.07f, 0.17f, 0.17f),
      new Ctrl("rb",   BTN,   PadState.RB,    "RB",               0.81f, 0.07f, 0.17f, 0.17f),
      new Ctrl("lt",   BTN,   PadState.LT,    "LT",               0.06f, 0.07f, 0.17f, 0.17f),
      new Ctrl("rt",   BTN,   PadState.RT,    "RT",               0.94f, 0.07f, 0.17f, 0.17f),
      new Ctrl("l3",   BTN,   PadState.L3,    "L3",               0.27f, 0.92f, 0.12f, 0.12f),
      new Ctrl("r3",   BTN,   PadState.R3,    "R3",               0.73f, 0.92f, 0.12f, 0.12f),
      new Ctrl("view", BTN,   PadState.VIEW,  "View",             0.39f, 0.06f, 0.13f, 0.13f),
      new Ctrl("xbox", BTN,   PadState.GUIDE, "Xbox",             0.50f, 0.06f, 0.13f, 0.13f),
      new Ctrl("menu", BTN,   PadState.MENU,  "Menu",             0.61f, 0.06f, 0.13f, 0.13f)
    };
  }

  private static Ctrl[] port() {
    return new Ctrl[] {
      new Ctrl("rs",   ZONE,  -1,             "Camera zone",      0.50f, 0.33f, 1.00f, 0.50f),
      new Ctrl("lsz",  LZONE, -1,             "Left stick area",  0.26f, 0.79f, 0.52f, 0.42f),
      new Ctrl("ls",   STICK, -1,             "Left stick",       0.22f, 0.84f, 0.34f, 0.34f),
      new Ctrl("dpad", DPAD,  -1,             "D-pad",            0.20f, 0.66f, 0.28f, 0.28f),
      new Ctrl("a",    BTN,   PadState.A,     "A",                0.80f, 0.84f, 0.16f, 0.16f),
      new Ctrl("b",    BTN,   PadState.B,     "B",                0.92f, 0.76f, 0.16f, 0.16f),
      new Ctrl("x",    BTN,   PadState.X,     "X",                0.67f, 0.76f, 0.16f, 0.16f),
      new Ctrl("y",    BTN,   PadState.Y,     "Y",                0.80f, 0.68f, 0.16f, 0.16f),
      new Ctrl("lb",   BTN,   PadState.LB,    "LB",               0.32f, 0.04f, 0.19f, 0.19f),
      new Ctrl("rb",   BTN,   PadState.RB,    "RB",               0.68f, 0.04f, 0.19f, 0.19f),
      new Ctrl("lt",   BTN,   PadState.LT,    "LT",               0.10f, 0.04f, 0.19f, 0.19f),
      new Ctrl("rt",   BTN,   PadState.RT,    "RT",               0.90f, 0.04f, 0.19f, 0.19f),
      new Ctrl("l3",   BTN,   PadState.L3,    "L3",               0.32f, 0.95f, 0.14f, 0.14f),
      new Ctrl("r3",   BTN,   PadState.R3,    "R3",               0.68f, 0.95f, 0.14f, 0.14f),
      new Ctrl("view", BTN,   PadState.VIEW,  "View",             0.35f, 0.61f, 0.15f, 0.15f),
      new Ctrl("xbox", BTN,   PadState.GUIDE, "Xbox",             0.50f, 0.61f, 0.15f, 0.15f),
      new Ctrl("menu", BTN,   PadState.MENU,  "Menu",             0.65f, 0.61f, 0.15f, 0.15f)
    };
  }

  /* what a control can be made to send, in gamepad terms */
  public static final int[] SENDS = {
    PadState.A, PadState.B, PadState.X, PadState.Y, PadState.LB, PadState.RB,
    PadState.LT, PadState.RT, PadState.VIEW, PadState.MENU, PadState.GUIDE,
    PadState.L3, PadState.R3, PadState.DU, PadState.DD, PadState.DL, PadState.DR
  };
  public static final String[] SEND_NAMES = {
    "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "View", "Menu", "Xbox",
    "L3 (stick click)", "R3 (stick click)", "D-pad up", "D-pad down", "D-pad left", "D-pad right"
  };
}
