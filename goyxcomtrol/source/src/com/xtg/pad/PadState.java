package com.xtg.pad;

/* The single source of truth for what the virtual controller is reporting.
   Touch handling writes into it, and both consumers read from it:
   - WebView mode: the injected JavaScript pulls a compact state snapshot
   - Shizuku mode: a 15 byte HID report is built from it

   The camera path intentionally avoids a long history window. We estimate velocity from
   the newest digitiser sample, apply only a very small one-pole filter, and let the
   stop threshold decide how long that velocity may survive a late web-frame. */
public final class PadState {

  public static final int A = 0, B = 1, X = 2, Y = 3, LB = 4, RB = 5, LT = 6, RT = 7,
      VIEW = 8, MENU = 9, L3 = 10, R3 = 11, DU = 12, DD = 13, DL = 14, DR = 15, GUIDE = 16;
  public static final int NB = 17;

  public final boolean[] btn = new boolean[NB];
  public volatile float lx, ly, rx, ry;     // -1..1
  public volatile float lt, rt;             // 0..1, analog
  public volatile boolean connected;

  /* ---- tunables, persisted ---- */
  public volatile int sens = 175;           // right stick, % (same numbers as the extension)
  public volatile int sensY = 135;
  public volatile int stillMs = 53;         // user's sweet spot: late-frame coast threshold
  public volatile boolean invertY = false;
  public volatile int deadzone = 0;         // left stick, %
  public volatile float density = 2.75f;    // px per dp, so sensitivity means the same thing
                                            // on any screen
  public volatile boolean kbm = false;      // send mouse and keys to the page instead
  public volatile int mouseSens = 140;      // % of 1:1 for mouse mode
  public volatile long lastActive = 0;      // for auto-hide

  /* ---- mouse + keyboard queue, drained by the page -----------------------------
     In mouse mode the camera is not a stick at all: the pixels your thumb moved become
     the same number of mouse pixels. There is no deflection, so no cap on turn speed and
     no rate for the game to integrate. The queue exists only because the page has to do the
     dispatching, and it is drained whole on every read. */
  public static final String[] KBM_DEF = new String[NB];
  static {
    KBM_DEF[A] = "k:Space";      KBM_DEF[B] = "k:ControlLeft";
    KBM_DEF[X] = "k:KeyR";       KBM_DEF[Y] = "k:KeyF";
    KBM_DEF[LB] = "k:KeyQ";      KBM_DEF[RB] = "k:KeyG";
    KBM_DEF[LT] = "b:2";         KBM_DEF[RT] = "b:0";
    KBM_DEF[VIEW] = "k:Tab";     KBM_DEF[MENU] = "k:Escape";  KBM_DEF[GUIDE] = "k:Escape";
    KBM_DEF[L3] = "k:ShiftLeft"; KBM_DEF[R3] = "k:KeyV";
    KBM_DEF[DU] = "k:ArrowUp";   KBM_DEF[DD] = "k:ArrowDown";
    KBM_DEF[DL] = "k:ArrowLeft"; KBM_DEF[DR] = "k:ArrowRight";
  }
  private final StringBuilder q = new StringBuilder(256);
  private float mdx = 0, mdy = 0;
  private final boolean[] wasd = new boolean[4];
  private static final String[] WASD = { "KeyW", "KeyS", "KeyA", "KeyD" };

  public synchronized void kbmMouse(float dx, float dy) {
    float g = mouseSens / 100f, gy = g * sensY / 100f;
    mdx += dx * g;
    mdy += dy * gy * (invertY ? -1 : 1);
  }

  public synchronized void kbmSend(int idx, boolean down) {
    String b = KBM_DEF[idx];
    if (b == null) return;
    if (q.length() > 0) q.append('|');
    q.append(b.charAt(0) == 'k' ? 'k' : 'b').append(down ? '+' : '-').append(b.substring(2));
  }

  /** left stick becomes WASD, pressed and released only on a real change of direction */
  public synchronized void kbmStick(float ax, float ay) {
    float t = 0.38f;
    boolean[] want = { ay < -t, ay > t, ax < -t, ax > t };
    for (int i = 0; i < 4; i++) {
      if (want[i] == wasd[i]) continue;
      wasd[i] = want[i];
      if (q.length() > 0) q.append('|');
      q.append('k').append(want[i] ? '+' : '-').append(WASD[i]);
    }
  }

  public synchronized void kbmReleaseAll() {
    for (int i = 0; i < 4; i++) wasd[i] = false;
    if (q.length() > 0) q.append('|');
    q.append('r');
  }

  /** "<dx>,<dy>|cmd|cmd" and the queue is emptied */
  public synchronized String kbmTake() {
    String out = ((int) mdx) + "," + ((int) mdy) + (q.length() > 0 ? "|" + q : "");
    mdx -= (int) mdx;
    mdy -= (int) mdy;
    q.setLength(0);
    return out;
  }

  public float touchInterval() { return interval; }

  /* ---- camera --------------------------------------------------------------- */
  private long prevT = 0;
  private long lastMove = 0;
  private float interval = 8f;
  private float vx = 0f, vy = 0f;
  private boolean camDown = false;

  /*
   * Precision touch camera:
   *
   * The touch surface is a relative pointing device. Each digitiser sample is
   * converted directly into a right-stick velocity, with NO smoothing, square-root
   * curve, or multi-sample history. Those operations change the geometric relationship
   * between the path of the finger and the path of the camera.
   *
   * The timestamp is used only to make the response invariant to touch sample rate.
   * The last non-zero velocity is held between samples, then cut after exactly the
   * user's 53 ms watchdog threshold. A delayed (>40 ms) sample is discarded rather
   * than turned into a fling.
   */
  public void camDown(long tMs) {
    camDown = true;
    prevT = tMs;
    lastMove = tMs;
    vx = vy = 0f;
    rx = ry = 0f;
  }

  public void camSample(float dx, float dy, long tMs) {
    if (!camDown) return;

    long dtMs = tMs - prevT;
    if (dtMs <= 0) dtMs = 1;
    prevT = tMs;

    if (dtMs <= 40L) {
      interval += (dtMs - interval) * 0.18f;
    }

    if (dx == 0f && dy == 0f) return;

    if (dtMs > 40L) {
      // Do not convert a stalled event into an unpredictable camera jump.
      vx = vy = 0f;
      rx = ry = 0f;
      return;
    }

    lastMove = tMs;

    float dens = density <= 0f ? 1f : density;

    // Keep the familiar sensitivity scale around the 1.0 reference at 175%.
    // The response itself is strictly linear, so a circle stays a circle and
    // direction follows the finger instead of being warped by a curve.
    float gainX = (sens / 175f) * 0.42f;
    float gainY = gainX * sensY / 100f;

    float tx = ((dx / dens) / dtMs) * gainX;
    float ty = ((dy / dens) / dtMs) * gainY;
    if (invertY) ty = -ty;

    vx = tx;
    vy = ty;
    rx = clamp(tx);
    ry = clamp(ty);
  }

  public void camUp() {
    camDown = false;
    rx = ry = 0f;
    vx = vy = 0f;
  }

  public void camCompute(long nowMs) {
    if (!camDown) {
      rx = ry = 0f;
      return;
    }

    // Exactly 53 ms by default: this is only a watchdog, never a sensitivity term.
    if (nowMs - lastMove >= stillMs) {
      rx = ry = 0f;
      return;
    }

    rx = clamp(vx);
    ry = clamp(vy);
  }

  private static float clamp(float v) {
    return v < -1f ? -1f : (v > 1f ? 1f : v);
  }

  /* ---- legacy wire format for compatibility/debug -------------------------------- */
  private final StringBuilder sb = new StringBuilder(160);
  public String wire() {
    sb.setLength(0);
    sb.append(connected ? '1' : '0').append(';');
    sb.append((int) (lx * 1000)).append(',').append((int) (ly * 1000)).append(',')
      .append((int) (rx * 1000)).append(',').append((int) (ry * 1000)).append(';');
    for (int i = 0; i < NB; i++) {
      if (i > 0) sb.append(',');
      float v = btn[i] ? 1f : 0f;
      if (i == LT) v = lt; else if (i == RT) v = rt;
      sb.append((int) (v * 1000));
    }
    return sb.toString();
  }

  /* ---- compact WebView snapshot ----------------------------------------------------
     9 UTF-16 code units:
       0 connected
       1..4 axes, 12-bit unsigned each
       5..6 LT/RT, 10-bit unsigned each
       7..8 button bits, low 16 + high 1
     This avoids JS split()/substring()/number-array allocations on every poll. */
  private final char[] fast = new char[9];
  public String fastWire() {
    fast[0] = (char) (connected ? 1 : 0);
    fast[1] = (char) axis12(lx);
    fast[2] = (char) axis12(ly);
    fast[3] = (char) axis12(rx);
    fast[4] = (char) axis12(ry);
    fast[5] = (char) trigger10(lt);
    fast[6] = (char) trigger10(rt);
    int bits0 = 0, bits1 = 0;
    for (int i = 0; i < 16; i++) if (btn[i]) bits0 |= 1 << i;
    if (btn[16]) bits1 = 1;
    fast[7] = (char) bits0;
    fast[8] = (char) bits1;
    return new String(fast);
  }

  private static int axis12(float v) {
    float c = clamp(v);
    int o = Math.round((c + 1f) * 2047.5f);
    return o < 0 ? 0 : (o > 4095 ? 4095 : o);
  }

  private static int trigger10(float v) {
    float c = clamp01(v);
    int o = Math.round(c * 1023f);
    return o < 0 ? 0 : (o > 1023 ? 1023 : o);
  }

  /* ---- HID report, 15 bytes, laid out to match the descriptor in UHid ------------
     X Y Rx Ry Z Rz as 16 bit unsigned, hat nibble, then 15 button bits. */
  private final byte[] rep = new byte[15];
  public byte[] report() {
    put16(0, axis16(lx)); put16(2, axis16(ly));
    put16(4, axis16(rx)); put16(6, axis16(ry));
    put16(8, (int) (clamp01(lt) * 65535f)); put16(10, (int) (clamp01(rt) * 65535f));
    boolean u = btn[DU], d = btn[DD], l = btn[DL], r = btn[DR];
    int hat = 0;
    if (u && r) hat = 2; else if (r && d) hat = 4; else if (d && l) hat = 6;
    else if (l && u) hat = 8; else if (u) hat = 1; else if (r) hat = 3;
    else if (d) hat = 5; else if (l) hat = 7;
    rep[12] = (byte) hat;
    int b = 0;
    if (btn[A]) b |= 1 << 0;
    if (btn[B]) b |= 1 << 1;
    if (btn[X]) b |= 1 << 3;
    if (btn[Y]) b |= 1 << 4;
    if (btn[LB]) b |= 1 << 6;
    if (btn[RB]) b |= 1 << 7;
    if (btn[VIEW]) b |= 1 << 10;
    if (btn[MENU]) b |= 1 << 11;
    if (btn[GUIDE]) b |= 1 << 12;
    if (btn[L3]) b |= 1 << 13;
    if (btn[R3]) b |= 1 << 14;
    rep[13] = (byte) (b & 0xFF);
    rep[14] = (byte) ((b >> 8) & 0x7F);
    return rep;
  }

  private static float clamp01(float v) { return v < 0 ? 0 : (v > 1 ? 1 : v); }
  private static int axis16(float v) {
    int o = Math.round((clamp(v) + 1f) * 32767.5f);
    return o < 0 ? 0 : (o > 65535 ? 65535 : o);
  }
  private void put16(int i, int v) { rep[i] = (byte) (v & 0xFF); rep[i + 1] = (byte) ((v >> 8) & 0xFF); }
}
