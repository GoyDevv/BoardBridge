package com.goydevv.inputbridge;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

public final class ControlModel {
    public static final int BUTTON = 0, JOYSTICK = 1, DPAD = 2, DRAWER = 3;
    public static final int KEY = 0, MOUSE_BUTTON = 1, SCROLL = 2;
    public static final int HOLD = 0, TOGGLE = 1, CLICK = 2;

    public static final class Action {
        int type = KEY;
        int code = 0;
        int value = 0;
        int behavior = HOLD;

        JSONObject json() throws Exception {
            JSONObject o = new JSONObject();
            o.put("type", type); o.put("code", code); o.put("value", value); o.put("behavior", behavior);
            return o;
        }
        static Action from(JSONObject o) {
            Action a = new Action();
            a.type=o.optInt("type",KEY); a.code=o.optInt("code",0);
            a.value=o.optInt("value",0); a.behavior=o.optInt("behavior",HOLD);
            return a;
        }
    }

    public static final class Control {
        String id = UUID.randomUUID().toString();
        String label = "BUTTON";
        int type = BUTTON;
        float x=.5f,y=.5f,w=.15f,h=.1f,opacity=.72f;
        boolean visible=true, open=true;
        final List<Action> actions = new ArrayList<>();
        final List<String> children = new ArrayList<>();

        JSONObject json() throws Exception {
            JSONObject o=new JSONObject();
            o.put("id",id);o.put("label",label);o.put("type",type);
            o.put("x",x);o.put("y",y);o.put("w",w);o.put("h",h);o.put("opacity",opacity);
            o.put("visible",visible);o.put("open",open);
            JSONArray a=new JSONArray();for(Action i:actions)a.put(i.json());o.put("actions",a);
            JSONArray c=new JSONArray();for(String i:children)c.put(i);o.put("children",c);
            return o;
        }

        static Control from(JSONObject o) {
            Control c=new Control();
            c.id=o.optString("id",c.id);c.label=o.optString("label","BUTTON");
            c.type=o.optInt("type",BUTTON);c.x=(float)o.optDouble("x",.5);c.y=(float)o.optDouble("y",.5);
            c.w=(float)o.optDouble("w",.15);c.h=(float)o.optDouble("h",.1);
            c.opacity=(float)o.optDouble("opacity",.72);c.visible=o.optBoolean("visible",true);c.open=o.optBoolean("open",true);
            JSONArray a=o.optJSONArray("actions");if(a!=null)for(int i=0;i<a.length();i++)if(a.optJSONObject(i)!=null)c.actions.add(Action.from(a.optJSONObject(i)));
            JSONArray ch=o.optJSONArray("children");if(ch!=null)for(int i=0;i<ch.length();i++)c.children.add(ch.optString(i));
            return c;
        }
    }

    private static final String PREF="xcloud_controls", KEY_LAYOUT="layout";

    public static List<Control> load(Context c) {
        List<Control> out=new ArrayList<>();
        String raw=c.getSharedPreferences(PREF,Context.MODE_PRIVATE).getString(KEY_LAYOUT,"");
        try {
            JSONArray a=new JSONArray(raw);
            for(int i=0;i<a.length();i++)if(a.optJSONObject(i)!=null)out.add(Control.from(a.optJSONObject(i)));
        } catch(Exception ignored) {}
        return out.isEmpty()?defaults():out;
    }

    public static void save(Context c,List<Control> controls) {
        JSONArray a=new JSONArray();
        try { for(Control x:controls)a.put(x.json()); } catch(Exception ignored) {}
        c.getSharedPreferences(PREF,Context.MODE_PRIVATE).edit().putString(KEY_LAYOUT,a.toString()).apply();
    }

    public static List<Control> defaults() {
        List<Control> out=new ArrayList<>();
        Control joy=new Control();joy.label="MOVE";joy.type=JOYSTICK;joy.x=.16f;joy.y=.77f;joy.w=.22f;joy.h=.22f;
        joy.actions.add(key(29));joy.actions.add(key(32));joy.actions.add(key(51));joy.actions.add(key(47));out.add(joy);
        Control fire=new Control();fire.label="FIRE";fire.x=.87f;fire.y=.77f;fire.w=.12f;fire.h=.11f;fire.actions.add(mouse(MotionEventConstants.LEFT));out.add(fire);
        Control aim=new Control();aim.label="AIM";aim.x=.73f;aim.y=.77f;aim.w=.12f;aim.h=.11f;aim.actions.add(mouse(MotionEventConstants.RIGHT));out.add(aim);
        Control jump=new Control();jump.label="JUMP";jump.x=.60f;jump.y=.89f;jump.w=.17f;jump.h=.08f;jump.actions.add(key(62));out.add(jump);
        return out;
    }

    static Action key(int code){Action a=new Action();a.type=KEY;a.code=code;return a;}
    static Action mouse(int code){Action a=new Action();a.type=MOUSE_BUTTON;a.code=code;return a;}

    static final class MotionEventConstants {
        static final int LEFT=1, RIGHT=2;
    }
}
