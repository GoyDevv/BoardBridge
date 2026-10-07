package com.xtg.pad;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.os.SystemClock;
import android.util.SparseArray;
import android.view.MotionEvent;
import android.view.View;

/* The overlay. Touch is read here and nowhere else.

   Two things decide how this feels. First, every pointer sample is read, including the
   historical ones inside a batched MotionEvent, each with its own event time - read only
   the latest position and you throw away most of a 240Hz digitiser, which makes the
   camera estimator guess. Second, nothing waits for a frame: the state is updated inside
   the touch event and whoever polls it gets the newest answer. */
public final class PadView extends View {

  private final PadState st;
  private Ctrl[] ctrls;
  private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
  private final Paint line = new Paint(Paint.ANTI_ALIAS_FLAG);
  private final Paint text = new Paint(Paint.ANTI_ALIAS_FLAG);
  private final RectF rf = new RectF();

  private final SparseArray<Ctrl> owner = new SparseArray<>();
  private final SparseArray<float[]> origin = new SparseArray<>();
  private int camPointer = -1, lsPointer = -1;
  private float camX, camY;
  private float lsCx, lsCy;              // where a floating left stick was planted
  private boolean lsFloating = false;

  public boolean edit = false;
  public Ctrl selected = null;
  public int opacity = 55;
  public boolean lite = false;
  public boolean hud = false;
  public boolean floatStick = true;      // left stick appears under your thumb
  public int autoHideS = 0;              // 0 = never
  public Runnable onSelect = null;
  public Mat mat;

  public PadView(Context c, PadState st, Ctrl[] ctrls) {
    super(c);
    this.st = st; this.ctrls = ctrls;
    this.mat = new Mat(c);
    setFocusable(false);
    line.setStyle(Paint.Style.STROKE);
    text.setTextAlign(Paint.Align.CENTER);
  }

  public void setCtrls(Ctrl[] c) { ctrls = c; selected = null; invalidate(); }
  public Ctrl[] ctrls() { return ctrls; }
  public Ctrl byId(String id) { for (Ctrl c : ctrls) if (c.id.equals(id)) return c; return null; }

  /* ---------------------------- geometry ---------------------------- */
  private float unit() { return Math.min(getWidth(), getHeight()); }
  private float cx(Ctrl c) { return c.x * getWidth(); }
  private float cy(Ctrl c) { return c.y * getHeight(); }
  private float rad(Ctrl c) { return c.w * unit() * 0.5f; }

  private boolean inside(Ctrl c, float x, float y) {
    if (c.kind == Ctrl.ZONE || c.kind == Ctrl.LZONE) {
      float hw = c.w * getWidth() * 0.5f, hh = c.h * getHeight() * 0.5f;
      return Math.abs(x - cx(c)) <= hw && Math.abs(y - cy(c)) <= hh;
    }
    float r = rad(c) * 1.14f;            // a shade generous, as on a real pad
    float dx = x - cx(c), dy = y - cy(c);
    if (c.square) return Math.abs(dx) <= r && Math.abs(dy) <= r;
    return dx * dx + dy * dy <= r * r;
  }

  private Ctrl hit(float x, float y) {
    Ctrl zone = null, lzone = null;
    for (Ctrl c : ctrls) {
      if (!c.on && !edit) continue;
      if (c.kind == Ctrl.ZONE) { if (inside(c, x, y)) zone = c; continue; }
      if (c.kind == Ctrl.LZONE) { if (inside(c, x, y)) lzone = c; continue; }
      if (c.kind == Ctrl.STICK && floatStick && !edit) continue;   // the area owns it
      if (inside(c, x, y)) return c;
    }
    if (lzone != null && (floatStick || edit)) return lzone;       // areas are claimed last
    return zone;
  }

  /* ---------------------------- touch ---------------------------- */
  @Override public boolean onTouchEvent(MotionEvent e) {
    int act = e.getActionMasked();
    st.lastActive = SystemClock.uptimeMillis();
    switch (act) {
      case MotionEvent.ACTION_DOWN:
      case MotionEvent.ACTION_POINTER_DOWN: {
        int idx = e.getActionIndex(), id = e.getPointerId(idx);
        float x = e.getX(idx), y = e.getY(idx);
        Ctrl c = hit(x, y);
        if (c == null) return false;                               // the page keeps it
        if (edit) {
          selected = c;
          origin.put(id, new float[]{ x, y, c.x, c.y });
          owner.put(id, c);
          if (onSelect != null) onSelect.run();
          invalidate();
          return true;
        }
        owner.put(id, c);
        press(c, id, x, y, e.getEventTime());
        invalidate();
        return true;
      }
      case MotionEvent.ACTION_MOVE: {
        int np = e.getPointerCount();
        for (int h = 0; h < e.getHistorySize(); h++) {
          long t = e.getHistoricalEventTime(h);
          for (int i = 0; i < np; i++)
            move(e.getPointerId(i), e.getHistoricalX(i, h), e.getHistoricalY(i, h), t);
        }
        long t = e.getEventTime();
        for (int i = 0; i < np; i++) move(e.getPointerId(i), e.getX(i), e.getY(i), t);

        // The camera zone is invisible while playing. Redrawing the full-screen
        // overlay for every camera sample needlessly competes with WebView/XCloud.
        // Only redraw when a visible control actually changed.
        boolean redraw = edit;
        if (!redraw) {
          for (int i = 0; i < owner.size(); i++) {
            Ctrl oc = owner.valueAt(i);
            if (oc != null && oc.kind != Ctrl.ZONE) { redraw = true; break; }
          }
        }
        if (redraw) invalidate();
        return true;
      }
      case MotionEvent.ACTION_UP:
      case MotionEvent.ACTION_POINTER_UP: {
        release(e.getPointerId(e.getActionIndex()), e);
        invalidate();
        return true;
      }
      case MotionEvent.ACTION_CANCEL: {
        for (int i = 0; i < owner.size(); i++) releaseCtrl(owner.valueAt(i));
        owner.clear(); origin.clear();
        camPointer = -1; lsPointer = -1; lsFloating = false;
        st.camUp();
        if (st.kbm) st.kbmReleaseAll();
        invalidate();
        return true;
      }
    }
    return true;
  }

  private void press(Ctrl c, int id, float x, float y, long tNs) {
    switch (c.kind) {
      case Ctrl.ZONE:
        camPointer = id; camX = x; camY = y;
        if (!st.kbm) st.camDown(tNs);
        break;
      case Ctrl.LZONE: {
        Ctrl s = byId("ls");
        if (s == null) return;
        lsPointer = id; lsFloating = true; lsCx = x; lsCy = y;
        owner.put(id, c);
        break;
      }
      case Ctrl.STICK:
        lsPointer = id; lsFloating = false;
        lsCx = cx(c); lsCy = cy(c);
        stick(c, x, y);
        break;
      case Ctrl.DPAD:
        dpad(c, x, y);
        break;
      default:
        hold(c, 1f, true);
    }
  }

  private void move(int id, float x, float y, long tMs) {
    Ctrl c = owner.get(id);
    if (c == null) return;
    if (edit) {
      float[] o = origin.get(id);
      if (o == null) return;
      c.x = clampF(o[2] + (x - o[0]) / getWidth());
      c.y = clampF(o[3] + (y - o[1]) / getHeight());
      return;
    }
    switch (c.kind) {
      case Ctrl.ZONE: {
        if (id != camPointer) return;
        float dx = x - camX, dy = y - camY;
        camX = x; camY = y;
        if (st.kbm) st.kbmMouse(dx, dy);
        else st.camSample(dx, dy, tMs);
        break;
      }
      case Ctrl.LZONE:
      case Ctrl.STICK: {
        if (id != lsPointer) return;
        Ctrl s = byId("ls");
        float r = rad(s == null ? c : s);
        float dx = (x - lsCx) / r, dy = (y - lsCy) / r;
        setStick(dx, dy);
        break;
      }
      case Ctrl.DPAD:
        dpad(c, x, y);
        break;
      default:
        if (c.analog && (c.btn == PadState.LT || c.btn == PadState.RT)) {
          // drag up the trigger for partial travel - a real trigger is not a switch
          float r = rad(c);
          float t = 1f - (cy(c) - y) / (r * 3f);
          hold(c, t < 0.08f ? 0.08f : (t > 1f ? 1f : t), true);
        }
    }
  }

  private void release(int id, MotionEvent e) {
    Ctrl c = owner.get(id);
    owner.remove(id); origin.remove(id);
    if (c == null) return;
    if (edit) return;
    if (c.kind == Ctrl.ZONE) {
      if (id != camPointer) return;
      // multi touch handover: if another finger is still in the zone the camera carries
      // on from it instead of snapping to a stop
      for (int i = 0; i < owner.size(); i++) {
        if (owner.valueAt(i) != c) continue;
        int other = owner.keyAt(i);
        int oi = e.findPointerIndex(other);
        if (oi < 0) continue;
        camPointer = other; camX = e.getX(oi); camY = e.getY(oi);
        if (!st.kbm) st.camDown(e.getEventTime());
        return;
      }
      camPointer = -1;
      st.camUp();
      return;
    }
    if (c.kind == Ctrl.LZONE || c.kind == Ctrl.STICK) {
      if (id != lsPointer) return;
      lsPointer = -1; lsFloating = false;
      setStick(0, 0);
      return;
    }
    releaseCtrl(c);
  }

  private void setStick(float dx, float dy) {
    float m = (float) Math.sqrt(dx * dx + dy * dy);
    if (m > 1f) { dx /= m; dy /= m; m = 1f; }
    float dz = st.deadzone / 100f;
    if (m < dz) { dx = 0; dy = 0; }
    else if (dz > 0 && m > 0) { float k = (m - dz) / (1f - dz) / m; dx *= k; dy *= k; }
    st.lx = dx; st.ly = dy;
    if (st.kbm) st.kbmStick(dx, dy);
  }

  private void hold(Ctrl c, float v, boolean down) {
    if (c.btn < 0) return;
    if (c.btn == PadState.LT) st.lt = down ? v : 0f;
    else if (c.btn == PadState.RT) st.rt = down ? v : 0f;
    boolean was = st.btn[c.btn];
    st.btn[c.btn] = down;
    if (st.kbm && was != down) st.kbmSend(c.btn, down);
  }

  private void releaseCtrl(Ctrl c) {
    if (c == null) return;
    if (c.kind == Ctrl.STICK || c.kind == Ctrl.LZONE) setStick(0, 0);
    else if (c.kind == Ctrl.DPAD) {
      for (int i : new int[]{ PadState.DU, PadState.DD, PadState.DL, PadState.DR }) {
        if (st.btn[i] && st.kbm) st.kbmSend(i, false);
        st.btn[i] = false;
      }
    } else if (c.kind == Ctrl.BTN) hold(c, 0f, false);
  }

  private void stick(Ctrl c, float x, float y) {
    float r = rad(c);
    setStick((x - cx(c)) / r, (y - cy(c)) / r);
  }

  private void dpad(Ctrl c, float x, float y) {
    float dx = x - cx(c), dy = y - cy(c), r = rad(c);
    boolean u = false, d = false, l = false, rr = false;
    if (Math.sqrt(dx * dx + dy * dy) > r * 0.22f) {
      double deg = (Math.atan2(dy, dx) * 180 / Math.PI + 360) % 360;
      if (deg > 202.5 && deg < 337.5) u = true;
      if (deg > 22.5 && deg < 157.5) d = true;
      if (deg > 112.5 && deg < 247.5) l = true;
      if (deg > 292.5 || deg < 67.5) rr = true;
    }
    boolean[] want = { u, d, l, rr };
    int[] idx = { PadState.DU, PadState.DD, PadState.DL, PadState.DR };
    for (int i = 0; i < 4; i++) {
      if (st.btn[idx[i]] == want[i]) continue;
      st.btn[idx[i]] = want[i];
      if (st.kbm) st.kbmSend(idx[i], want[i]);
    }
  }

  private static float clampF(float v) { return v < 0.02f ? 0.02f : (v > 0.98f ? 0.98f : v); }

  /* ---------------------------- drawing ---------------------------- */
  @Override protected void onDraw(Canvas cv) {
    float u = unit();
    int base = (int) (255 * (opacity / 100f));
    if (autoHideS > 0 && !edit) {
      float idle = (SystemClock.uptimeMillis() - st.lastActive) / 1000f;
      if (idle > autoHideS) {
        float f = Math.max(0f, 1f - (idle - autoHideS) / 0.6f);
        base = (int) (base * f);
        if (base <= 2) { drawHud(cv, u); return; }
        postInvalidateDelayed(80);
      } else postInvalidateDelayed((long) ((autoHideS - idle) * 1000) + 60);
    }
    int accent = mat.accent;
    for (Ctrl c : ctrls) {
      if (!c.on && !edit) continue;
      boolean sel = edit && c == selected;
      int al = (int) (base * (c.op / 100f));
      if (c.kind == Ctrl.ZONE || c.kind == Ctrl.LZONE) {
        if (!edit) continue;
        float hw = c.w * getWidth() * 0.5f, hh = c.h * getHeight() * 0.5f;
        rf.set(cx(c) - hw, cy(c) - hh, cx(c) + hw, cy(c) + hh);
        line.setColor(sel ? accent : Mat.alpha(accent, 90));
        line.setStrokeWidth(u * (sel ? 0.008f : 0.005f));
        cv.drawRoundRect(rf, u * 0.025f, u * 0.025f, line);
        text.setColor(Mat.alpha(mat.text, 190));
        text.setTextSize(u * 0.04f);
        cv.drawText(c.name, cx(c), cy(c), text);
        continue;
      }
      if (c.kind == Ctrl.STICK && floatStick && !edit) continue;

      float r = rad(c);
      float x = cx(c), y = cy(c);
      if (c.kind == Ctrl.STICK && lsFloating) { x = lsCx; y = lsCy; }
      boolean active = c.kind == Ctrl.BTN ? (c.btn >= 0 && st.btn[c.btn])
          : (c.kind == Ctrl.STICK ? (st.lx != 0 || st.ly != 0) : false);

      if (!lite) {
        fill.setColor(active ? Mat.alpha(accent, Math.min(255, al + 90))
                             : Mat.alpha(mat.dark ? 0xFF0D1319 : 0xFF2A333C, al));
        if (c.square) {
          rf.set(x - r, y - r, x + r, y + r);
          cv.drawRoundRect(rf, r * 0.28f, r * 0.28f, fill);
        } else cv.drawCircle(x, y, r, fill);
      }
      line.setColor(sel ? accent : Mat.alpha(active ? accent : mat.text, Math.min(255, al + 70)));
      line.setStrokeWidth(u * (sel ? 0.008f : 0.0045f));
      if (c.square) {
        rf.set(x - r, y - r, x + r, y + r);
        cv.drawRoundRect(rf, r * 0.28f, r * 0.28f, line);
      } else cv.drawCircle(x, y, r, line);

      if (c.kind == Ctrl.DPAD) {
        line.setStrokeWidth(u * 0.004f);
        cv.drawLine(x - r * 0.62f, y, x + r * 0.62f, y, line);
        cv.drawLine(x, y - r * 0.62f, x, y + r * 0.62f, line);
        int[] idx = { PadState.DU, PadState.DD, PadState.DL, PadState.DR };
        float[][] at = { {0, -0.6f}, {0, 0.6f}, {-0.6f, 0}, {0.6f, 0} };
        for (int i = 0; i < 4; i++) {
          if (!st.btn[idx[i]]) continue;
          fill.setColor(Mat.alpha(accent, Math.min(255, al + 120)));
          cv.drawCircle(x + at[i][0] * r, y + at[i][1] * r, r * 0.2f, fill);
        }
      } else if (c.kind == Ctrl.STICK) {
        float kx = x + st.lx * r * 0.58f, ky = y + st.ly * r * 0.58f;
        fill.setColor(Mat.alpha(accent, Math.min(255, al + 120)));
        cv.drawCircle(kx, ky, r * 0.42f, fill);
      } else {
        String lab = c.label();
        text.setColor(Mat.alpha(active ? mat.onAccent : mat.text, Math.min(255, al + 110)));
        float size = Math.min(r * 0.9f, r * 1.7f / Math.max(1, lab.length() * 0.62f));
        text.setTextSize(size);
        cv.drawText(lab, x, y + size * 0.35f, text);
        if (c.analog && (c.btn == PadState.LT || c.btn == PadState.RT)) {
          float v = c.btn == PadState.LT ? st.lt : st.rt;
          if (v > 0) {
            fill.setColor(Mat.alpha(accent, 220));
            rf.set(x - r * 0.72f, y + r * 0.56f, x - r * 0.72f + r * 1.44f * v, y + r * 0.72f);
            cv.drawRoundRect(rf, r * 0.08f, r * 0.08f, fill);
          }
        }
      }
    }
    drawHud(cv, u);
  }

  private void drawHud(Canvas cv, float u) {
    if (!hud) return;
    text.setTextAlign(Paint.Align.LEFT);
    text.setColor(mat.accent);
    text.setTextSize(u * 0.025f);
    float lh = u * 0.03f, y = lh * 1.4f, x = u * 0.02f;
    cv.drawText((st.kbm ? "mouse+keys" : "pad") + "  touch " + fmt(st.touchInterval()) + "ms", x, y, text);
    cv.drawText("L " + fmt(st.lx) + "," + fmt(st.ly) + "   R " + fmt(st.rx) + "," + fmt(st.ry),
        x, y + lh, text);
    StringBuilder b = new StringBuilder();
    String[] n = { "A","B","X","Y","LB","RB","LT","RT","VW","MN","L3","R3","U","D","L","R","XB" };
    for (int i = 0; i < PadState.NB; i++) if (st.btn[i]) b.append(n[i]).append(' ');
    cv.drawText(b.length() == 0 ? "-" : b.toString(), x, y + lh * 2, text);
    text.setTextAlign(Paint.Align.CENTER);
    postInvalidateDelayed(100);
  }

  private static String fmt(float v) { return String.format("%.2f", v); }
}
