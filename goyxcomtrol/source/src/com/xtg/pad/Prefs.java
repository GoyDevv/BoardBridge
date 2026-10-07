package com.xtg.pad;

import android.content.Context;
import android.content.SharedPreferences;
import org.json.JSONArray;
import org.json.JSONObject;

public final class Prefs {
  private final SharedPreferences sp;
  public Prefs(Context c) { sp = c.getSharedPreferences("xtg", Context.MODE_PRIVATE); }

  public int getInt(String k, int d) { return sp.getInt(k, d); }
  public void putInt(String k, int v) { sp.edit().putInt(k, v).apply(); }
  public boolean getBool(String k, boolean d) { return sp.getBoolean(k, d); }
  public void putBool(String k, boolean v) { sp.edit().putBoolean(k, v).apply(); }
  public String getStr(String k, String d) { return sp.getString(k, d); }
  public void putStr(String k, String v) { sp.edit().putString(k, v).apply(); }

  private String key(String profile, boolean portrait) {
    return "lay_" + profile + (portrait ? "_p" : "_l");
  }

  public String layoutJson(String profile, Ctrl[] cs) {
    JSONArray a = new JSONArray();
    for (Ctrl c : cs) a.put(c.toJson());
    return a.toString();
  }

  public void saveLayout(String profile, boolean portrait, Ctrl[] cs) {
    sp.edit().putString(key(profile, portrait), layoutJson(profile, cs)).apply();
  }

  /** the stored layout for this profile and orientation, or the preset if there is none */
  public Ctrl[] loadLayout(String profile, boolean portrait) {
    Ctrl[] base = Layouts.preset(profile, portrait);
    String s = sp.getString(key(profile, portrait), null);
    if (s == null) return base;
    applyLayoutJson(s, base);
    return base;
  }

  public boolean applyLayoutJson(String json, Ctrl[] into) {
    if (json == null) return false;
    try {
      JSONArray a = new JSONArray(json.trim());
      int hit = 0;
      for (int i = 0; i < a.length(); i++) {
        JSONObject o = a.getJSONObject(i);
        String id = o.optString("id");
        for (Ctrl c : into) if (c.id.equals(id)) { c.fromJson(o); hit++; }
      }
      return hit > 0;
    } catch (Exception e) { return false; }
  }
}
