package com.goydevv.inputbridge;

interface IInputBridge {
    oneway void mouseMove(float dx, float dy, long eventTime);
    oneway void mouseButton(int button, boolean pressed, long eventTime);
    oneway void mouseScroll(float horizontal, float vertical, long eventTime);
    oneway void keyEvent(int keyCode, boolean pressed, int metaState, long eventTime);
    oneway void releaseAll();
    String getStatus();
    int getUid();
}
