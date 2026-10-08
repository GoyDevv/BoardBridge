package com.goydevv.inputbridge;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.res.Configuration;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.os.Build;
import android.os.IBinder;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class OverlayService extends Service {
    private WindowManager wm;
    private OverlayView view;
    private ToggleView toggle;
    private WindowManager.LayoutParams overlayParams;
    private WindowManager.LayoutParams toggleParams;
    private boolean controlsVisible = true;

    @Override public void onCreate() {
        super.onCreate();
        startForegroundCompat();
        wm=(WindowManager)getSystemService(WINDOW_SERVICE);
        controlsVisible=getSharedPreferences("xcloud_overlay",MODE_PRIVATE).getBoolean("visible",true);
        refreshWindows();
    }

    @Override public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        refreshWindows();
    }

    private boolean landscape() {
        return getResources().getConfiguration().orientation == Configuration.ORIENTATION_LANDSCAPE;
    }

    private void refreshWindows() {
        if (wm == null) return;
        if (!landscape()) {
            removeOverlay();
            removeToggle();
            return;
        }
        if (controlsVisible) {
            removeToggle();
            if (view == null) addOverlay();
        } else {
            removeOverlay();
            if (toggle == null) addToggle();
        }
    }

    private void addOverlay() {
        view=new OverlayView();
        overlayParams=new WindowManager.LayoutParams(
                WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.MATCH_PARENT,
                Build.VERSION.SDK_INT>=26 ? WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY : WindowManager.LayoutParams.TYPE_PHONE,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                        | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
                        | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
                android.graphics.PixelFormat.TRANSLUCENT);
        try { wm.addView(view,overlayParams); } catch(Throwable t) { view=null; }
    }

    private void addToggle() {
        toggle=new ToggleView();
        int size=dp(58);
        toggleParams=new WindowManager.LayoutParams(
                size,size,
                Build.VERSION.SDK_INT>=26 ? WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY : WindowManager.LayoutParams.TYPE_PHONE,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                        | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
                        | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
                android.graphics.PixelFormat.TRANSLUCENT);
        toggleParams.gravity=android.view.Gravity.TOP|android.view.Gravity.START;
        toggleParams.x=dp(14);
        toggleParams.y=dp(14);
        try { wm.addView(toggle,toggleParams); } catch(Throwable t) { toggle=null; }
    }

    private void removeOverlay() {
        if(view!=null) {
            view.releaseAll();
            try { wm.removeViewImmediate(view); } catch(Throwable ignored) {}
            view=null;
        }
    }

    private void removeToggle() {
        if(toggle!=null) {
            try { wm.removeViewImmediate(toggle); } catch(Throwable ignored) {}
            toggle=null;
        }
    }

    private void setControlsVisible(boolean visible) {
        controlsVisible=visible;
        getSharedPreferences("xcloud_overlay",MODE_PRIVATE).edit().putBoolean("visible",visible).apply();
        refreshWindows();
    }

    @Override public int onStartCommand(Intent intent,int flags,int id){return START_STICKY;}

    @Override public void onDestroy() {
        removeOverlay();
        removeToggle();
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent){return null;}

    private void startForegroundCompat(){
        String id="xcloud-input";
        NotificationManager nm=(NotificationManager)getSystemService(NOTIFICATION_SERVICE);
        if(Build.VERSION.SDK_INT>=26)nm.createNotificationChannel(
                new NotificationChannel(id,"Xcloud Input",NotificationManager.IMPORTANCE_LOW));
        Intent open=new Intent(this,MainActivity.class);
        PendingIntent pi=PendingIntent.getActivity(this,0,open,
                PendingIntent.FLAG_IMMUTABLE|PendingIntent.FLAG_UPDATE_CURRENT);
        Notification n=new Notification.Builder(this,id)
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setContentTitle("Xcloud Input Bridge")
                .setContentText("Landscape input overlay active")
                .setContentIntent(pi).setOngoing(true).build();
        if(Build.VERSION.SDK_INT>=29)
            startForeground(9,n,android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        else startForeground(9,n);
    }

    private int dp(int n){return Math.round(n*getResources().getDisplayMetrics().density);}

    private final class ToggleView extends View {
        private final Paint p=new Paint(Paint.ANTI_ALIAS_FLAG);
        ToggleView(){super(OverlayService.this);setLayerType(View.LAYER_TYPE_SOFTWARE,null);}
        @Override protected void onDraw(Canvas c){
            float d=getResources().getDisplayMetrics().density;
            p.setColor(Color.argb(220,20,22,28));
            c.drawRoundRect(new RectF(3,3,getWidth()-3,getHeight()-3),18*d,18*d,p);
            p.setStyle(Paint.Style.STROKE);p.setStrokeWidth(2*d);p.setColor(Color.argb(150,255,255,255));
            c.drawRoundRect(new RectF(3,3,getWidth()-3,getHeight()-3),18*d,18*d,p);
            p.setStyle(Paint.Style.FILL);p.setColor(Color.WHITE);p.setTextAlign(Paint.Align.CENTER);
            p.setTextSize(12*d);c.drawText("SHOW",getWidth()/2f,getHeight()/2f+4*d,p);
        }
        @Override public boolean onTouchEvent(MotionEvent e){
            if(e.getActionMasked()==MotionEvent.ACTION_UP){setControlsVisible(true);performClick();}
            return true;
        }
        @Override public boolean performClick(){super.performClick();return true;}
    }

    private final class OverlayView extends View {
        private final Paint p=new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Path cursorPath=new Path();
        private final BridgeClient bridge=BridgeClient.get(OverlayService.this);
        private final List<ControlModel.Control> controls=ControlModel.load(OverlayService.this);
        private final Map<Integer,ControlModel.Control> active=new HashMap<>();

        private float cursorX=.72f;
        private float cursorY=.50f;
        private float lastMouseX,lastMouseY;
        private int mousePointerId=-1;
        private boolean mousePadActive;

        OverlayView(){
            super(OverlayService.this);
            setBackgroundColor(Color.TRANSPARENT);
            setLayerType(View.LAYER_TYPE_SOFTWARE,null);
            setFocusable(false);
        }

        @Override protected void onDraw(Canvas c){
            super.onDraw(c);
            int w=getWidth(),h=getHeight();
            if(w<=h)return;

            p.setStyle(Paint.Style.FILL);
            p.setColor(Color.argb(34,255,255,255));
            c.drawRoundRect(new RectF(w*.49f,dp(12),w-dp(12),h*.70f),dp(22),dp(22),p);

            p.setColor(Color.argb(120,10,12,18));
            c.drawRoundRect(new RectF(dp(14),dp(14),dp(112),dp(58)),dp(18),dp(18),p);
            p.setColor(Color.WHITE);p.setTextAlign(Paint.Align.CENTER);p.setTextSize(dp(12));
            c.drawText("HIDE",dp(63),dp(41),p);

            for(ControlModel.Control x:controls){
                if(!x.visible)continue;
                float px=x.x*w,py=x.y*h,rw=x.w*w,rh=x.h*h;
                p.setStyle(Paint.Style.FILL);
                p.setColor(Color.argb(Math.max(30,Math.min(210,(int)(255*x.opacity))),45,48,58));
                if(x.type==ControlModel.JOYSTICK){
                    c.drawCircle(px,py,Math.min(rw,rh)*.5f,p);
                    p.setStyle(Paint.Style.STROKE);p.setStrokeWidth(dp(2));p.setColor(Color.argb(90,255,255,255));
                    c.drawCircle(px,py,Math.min(rw,rh)*.5f,p);
                    p.setStyle(Paint.Style.FILL);
                } else if(x.type==ControlModel.DPAD) {
                    c.drawRoundRect(new RectF(px-rw*.5f,py-rh*.5f,px+rw*.5f,py+rh*.5f),dp(18),dp(18),p);
                } else {
                    c.drawRoundRect(new RectF(px-rw*.5f,py-rh*.5f,px+rw*.5f,py+rh*.5f),dp(16),dp(16),p);
                }
                p.setColor(Color.WHITE);p.setTextSize(Math.max(dp(11),rh*.20f));p.setTextAlign(Paint.Align.CENTER);
                c.drawText(x.label,px,py+p.getTextSize()*.35f,p);
            }

            drawCursor(c,w,h);
        }

        private void drawCursor(Canvas c,int w,int h){
            float x=cursorX*w,y=cursorY*h;
            float s=dp(22);
            cursorPath.reset();
            cursorPath.moveTo(x,y);
            cursorPath.lineTo(x+s*.38f,y+s*1.05f);
            cursorPath.lineTo(x+s*.62f,y+s*.86f);
            cursorPath.lineTo(x+s*.88f,y+s*1.32f);
            cursorPath.lineTo(x+s*1.05f,y+s*1.20f);
            cursorPath.lineTo(x+s*.80f,y+s*.72f);
            cursorPath.lineTo(x+s*1.18f,y+s*.70f);
            cursorPath.close();
            p.setStyle(Paint.Style.FILL);
            p.setColor(Color.argb(235,255,255,255));c.drawPath(cursorPath,p);
            p.setStyle(Paint.Style.STROKE);p.setStrokeWidth(dp(2));p.setColor(Color.BLACK);c.drawPath(cursorPath,p);
            p.setStyle(Paint.Style.FILL);
        }

        @Override protected void onSizeChanged(int w,int h,int ow,int oh){
            if(w>h && cursorX<=0)cursorX=.72f;
            invalidate();
        }

        @Override public boolean onTouchEvent(MotionEvent e){
            int action=e.getActionMasked();
            int idx=e.getActionIndex();
            int pid=e.getPointerId(idx);
            float x=e.getX(idx),y=e.getY(idx);

            if(action==MotionEvent.ACTION_DOWN){
                if(isHideButton(x,y)){setControlsVisible(false);performClick();return true;}
                if(isMousePad(x,y)){
                    mousePadActive=true;mousePointerId=pid;lastMouseX=x;lastMouseY=y;return true;
                }
                ControlModel.Control c=find(x/getWidth(),y/getHeight());
                if(c!=null){active.put(pid,c);apply(c,true,x,y);return true;}
                return true;
            }

            if(action==MotionEvent.ACTION_POINTER_DOWN){
                if(isMousePad(x,y)){
                    mousePadActive=true;mousePointerId=pid;lastMouseX=x;lastMouseY=y;return true;
                }
                ControlModel.Control c=find(x/getWidth(),y/getHeight());
                if(c!=null){active.put(pid,c);apply(c,true,x,y);}
                return true;
            }

            if(action==MotionEvent.ACTION_MOVE){
                if(mousePadActive && mousePointerId>=0){
                    for(int i=0;i<e.getPointerCount();i++){
                        if(e.getPointerId(i)!=mousePointerId)continue;
                        float nx=e.getX(i),ny=e.getY(i);
                        float dx=nx-lastMouseX,dy=ny-lastMouseY;
                        lastMouseX=nx;lastMouseY=ny;
                        if(dx!=0f||dy!=0f){
                            cursorX=Math.max(0f,Math.min(0.985f,cursorX+dx/getWidth()));
                            cursorY=Math.max(0f,Math.min(0.985f,cursorY+dy/getHeight()));
                            bridge.mouseMove(dx,dy,SystemClock.uptimeMillis());
                            invalidate();
                        }
                        break;
                    }
                }
                for(int i=0;i<e.getPointerCount();i++){
                    ControlModel.Control c=active.get(e.getPointerId(i));
                    if(c!=null)updateControl(c,e.getX(i),e.getY(i));
                }
                return true;
            }

            if(action==MotionEvent.ACTION_UP || action==MotionEvent.ACTION_POINTER_UP || action==MotionEvent.ACTION_CANCEL){
                ControlModel.Control c=active.remove(pid);
                if(c!=null)releaseControl(c);
                if(pid==mousePointerId){mousePointerId=-1;mousePadActive=false;}
                if(action==MotionEvent.ACTION_CANCEL)releaseAll();
                return true;
            }
            return true;
        }

        private boolean isHideButton(float x,float y){return x<dp(125)&&y<dp(75);}
        private boolean isMousePad(float x,float y){
            return x>getWidth()*.49f && y>dp(70) && y<getHeight()*.72f;
        }

        private ControlModel.Control find(float x,float y){
            for(int i=controls.size()-1;i>=0;i--){
                ControlModel.Control c=controls.get(i);
                if(!c.visible||c.type==ControlModel.DRAWER)continue;
                if(Math.abs(x-c.x)<=c.w*.5f&&Math.abs(y-c.y)<=c.h*.5f)return c;
            }
            return null;
        }

        private void apply(ControlModel.Control c,boolean down,float x,float y){
            if(c.type==ControlModel.JOYSTICK||c.type==ControlModel.DPAD){updateControl(c,x,y);return;}
            for(ControlModel.Action a:c.actions)emit(a,down);
        }

        private void updateControl(ControlModel.Control c,float x,float y){
            if(c.actions.size()<4)return;
            float nx=(x/getWidth()-c.x)/(c.w*.5f);
            float ny=(y/getHeight()-c.y)/(c.h*.5f);
            int left=c.actions.get(0).code,right=c.actions.get(1).code;
            int up=c.actions.get(2).code,down=c.actions.get(3).code;
            long t=SystemClock.uptimeMillis();
            bridge.keyEvent(left,nx<-.25f,0,t);
            bridge.keyEvent(right,nx>.25f,0,t);
            bridge.keyEvent(up,ny<-.25f,0,t);
            bridge.keyEvent(down,ny>.25f,0,t);
        }

        private void releaseControl(ControlModel.Control c){
            if(c.type==ControlModel.JOYSTICK||c.type==ControlModel.DPAD){
                if(c.actions.size()>=4)
                    for(int i=0;i<4;i++)bridge.keyEvent(c.actions.get(i).code,false,0,SystemClock.uptimeMillis());
            } else for(ControlModel.Action a:c.actions)emit(a,false);
        }

        private void emit(ControlModel.Action a,boolean down){
            long t=SystemClock.uptimeMillis();
            if(a.type==ControlModel.KEY)bridge.keyEvent(a.code,down,0,t);
            else if(a.type==ControlModel.MOUSE_BUTTON)bridge.mouseButton(a.code,down,t);
            else if(a.type==ControlModel.SCROLL && down)bridge.mouseScroll(0,a.value,t);
        }

        @Override public boolean performClick(){super.performClick();return true;}
        void releaseAll(){bridge.releaseAll();active.clear();mousePadActive=false;mousePointerId=-1;}
    }
}
