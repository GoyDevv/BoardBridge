package com.goydevv.inputbridge;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.view.MotionEvent;
import android.view.View;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;

import java.util.List;

public final class ControlEditorActivity extends Activity {
    private List<ControlModel.Control> controls;
    private CanvasView canvas;

    @Override public void onCreate(Bundle state){
        super.onCreate(state);
        controls=ControlModel.load(this);
        LinearLayout root=new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.rgb(12,12,16));

        LinearLayout top=new LinearLayout(this);
        Button menu=button("ADD");
        menu.setOnClickListener(v->showAdd());
        Button save=button("SAVE");
        save.setOnClickListener(v->{ControlModel.save(this,controls);toast("saved");});
        Button defaults=button("DEFAULT");
        defaults.setOnClickListener(v->{controls=ControlModel.defaults();canvas.invalidate();});
        top.addView(menu,new LinearLayout.LayoutParams(0,dp(52),1));
        top.addView(save,new LinearLayout.LayoutParams(0,dp(52),1));
        top.addView(defaults,new LinearLayout.LayoutParams(0,dp(52),1));
        root.addView(top);

        canvas=new CanvasView();
        root.addView(canvas,new LinearLayout.LayoutParams(-1,0,1));

        TextView hint=new TextView(this);
        hint.setText("Drag • tap to edit • use ADD for button/joystick/d-pad/drawer");
        hint.setTextColor(Color.LTGRAY);
        hint.setTextSize(12);
        hint.setPadding(dp(10),dp(8),dp(10),dp(8));
        root.addView(hint);
        setContentView(root);
    }

    private void showAdd(){
        String[] items={"Button","Joystick","D-pad","Button Drawer"};
        new android.app.AlertDialog.Builder(this).setTitle("Add control").setItems(items,(d,w)->{
            if(w==0)addButton();else if(w==1)addJoystick();else if(w==2)addDpad();else addDrawer();
        }).show();
    }

    private void addButton(){
        ControlModel.Control c=new ControlModel.Control();c.label="NEW";c.actions.add(ControlModel.key(62));controls.add(c);canvas.invalidate();edit(c);
    }
    private void addJoystick(){
        ControlModel.Control c=new ControlModel.Control();c.label="MOVE";c.type=ControlModel.JOYSTICK;c.x=.18f;c.y=.75f;c.w=.22f;c.h=.22f;
        c.actions.add(ControlModel.key(29));c.actions.add(ControlModel.key(32));c.actions.add(ControlModel.key(51));c.actions.add(ControlModel.key(47));controls.add(c);canvas.invalidate();
    }
    private void addDpad(){
        ControlModel.Control c=new ControlModel.Control();c.label="DPAD";c.type=ControlModel.DPAD;c.x=.34f;c.y=.75f;c.w=.22f;c.h=.22f;
        c.actions.add(ControlModel.key(21));c.actions.add(ControlModel.key(22));c.actions.add(ControlModel.key(19));c.actions.add(ControlModel.key(20));controls.add(c);canvas.invalidate();
    }
    private void addDrawer(){
        ControlModel.Control c=new ControlModel.Control();c.label="DRAWER";c.type=ControlModel.DRAWER;c.x=.5f;c.y=.12f;c.w=.18f;c.h=.08f;controls.add(c);canvas.invalidate();
    }

    private void edit(ControlModel.Control c){
        LinearLayout box=new LinearLayout(this);box.setOrientation(LinearLayout.VERTICAL);
        EditText label=field("Label",c.label),x=field("X",String.valueOf(c.x)),y=field("Y",String.valueOf(c.y)),w=field("Width",String.valueOf(c.w)),h=field("Height",String.valueOf(c.h));
        box.addView(label);box.addView(x);box.addView(y);box.addView(w);box.addView(h);
        Button input=button("INPUTS ("+c.actions.size()+"/4)");
        input.setOnClickListener(v->chooseInput(c,input));
        box.addView(input);
        new android.app.AlertDialog.Builder(this).setTitle("Edit control").setView(box)
            .setPositiveButton("SAVE",(d,which)->{
                c.label=label.getText().toString();
                c.x=num(x,c.x);c.y=num(y,c.y);c.w=Math.max(.03f,num(w,c.w));c.h=Math.max(.03f,num(h,c.h));
                ControlModel.save(this,controls);canvas.invalidate();
            }).setNegativeButton("CANCEL",null).show();
    }

    private void chooseInput(ControlModel.Control c,Button status){
        if(c.actions.isEmpty()||c.actions.size()>4)return;
        int slot=c.actions.size()-1;
        String[] choices={"W","A","S","D","SPACE","SHIFT","CTRL","TAB","Mouse Left","Mouse Right","Mouse Middle","Wheel Up","Wheel Down"};
        new android.app.AlertDialog.Builder(this).setTitle("Input slot "+(slot+1)).setItems(choices,(d,w)->{
            ControlModel.Action a=c.actions.get(slot);
            if(w<8){int[] k={51,29,47,32,62,59,113,61};a.type=ControlModel.KEY;a.code=k[w];a.value=0;}
            else if(w<11){a.type=ControlModel.MOUSE_BUTTON;a.code=w-7;a.value=0;}
            else{a.type=ControlModel.SCROLL;a.code=0;a.value=w==11?1:-1;}
            status.setText("INPUTS ("+c.actions.size()+"/4)");
        }).show();
    }

    private EditText field(String hint,String value){EditText e=new EditText(this);e.setHint(hint);e.setText(value);return e;}
    private float num(EditText e,float def){try{return Float.parseFloat(e.getText().toString());}catch(Exception ex){return def;}}

    private final class CanvasView extends View {
        private final android.graphics.Paint p=new android.graphics.Paint(1);
        private ControlModel.Control active;
        CanvasView(){super(ControlEditorActivity.this);p.setTextAlign(android.graphics.Paint.Align.CENTER);}
        @Override protected void onDraw(android.graphics.Canvas c){
            c.drawColor(Color.rgb(22,22,28));
            for(ControlModel.Control x:controls){
                float px=x.x*getWidth(),py=x.y*getHeight(),rw=x.w*getWidth(),rh=x.h*getHeight();
                p.setColor(Color.argb((int)(255*x.opacity),72,78,95));
                if(x.type==ControlModel.JOYSTICK||x.type==ControlModel.DPAD)c.drawCircle(px,py,Math.min(rw,rh)/2,p);
                else c.drawRoundRect(px-rw/2,py-rh/2,px+rw/2,py+rh/2,14,14,p);
                p.setColor(Color.WHITE);p.setTextSize(dp(12));c.drawText(x.label,px,py+dp(4),p);
            }
        }
        @Override public boolean onTouchEvent(MotionEvent e){
            float nx=e.getX()/getWidth(),ny=e.getY()/getHeight();
            if(e.getActionMasked()==MotionEvent.ACTION_DOWN){active=find(nx,ny);return true;}
            if(e.getActionMasked()==MotionEvent.ACTION_MOVE&&active!=null){active.x=Math.max(0,Math.min(1,nx));active.y=Math.max(0,Math.min(1,ny));invalidate();return true;}
            if(e.getActionMasked()==MotionEvent.ACTION_UP&&active!=null){edit(active);active=null;return true;}
            return true;
        }
        private ControlModel.Control find(float x,float y){
            for(int i=controls.size()-1;i>=0;i--){ControlModel.Control c=controls.get(i);if(Math.abs(x-c.x)<=c.w/2&&Math.abs(y-c.y)<=c.h/2)return c;}
            return null;
        }
    }

    private Button button(String s){Button b=new Button(this);b.setText(s);b.setAllCaps(false);return b;}
    private int dp(int n){return Math.round(n*getResources().getDisplayMetrics().density);}
    private void toast(String s){android.widget.Toast.makeText(this,s,android.widget.Toast.LENGTH_SHORT).show();}
}
