package com.rendongfang.adbmonster.vpnhelper;

import android.app.Activity;
import android.content.Intent;
import android.net.VpnService;
import android.os.Build;
import android.os.Bundle;
import android.view.Gravity;
import android.widget.TextView;
import android.widget.Toast;

import hev.sockstun.TProxyService;

public final class ControlActivity extends Activity {
    public static final String ACTION_AUTHORIZE =
            "com.rendongfang.adbmonster.vpnhelper.AUTHORIZE";
    public static final String ACTION_APPLY =
            "com.rendongfang.adbmonster.vpnhelper.APPLY";
    public static final String ACTION_STOP =
            "com.rendongfang.adbmonster.vpnhelper.STOP";

    private static final int VPN_PERMISSION_REQUEST = 42;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        TextView hint = new TextView(this);
        int padding = Math.round(24 * getResources().getDisplayMetrics().density);
        hint.setPadding(padding, padding, padding, padding);
        hint.setGravity(Gravity.CENTER);
        hint.setText(R.string.permission_hint);
        setContentView(hint);

        String action = getIntent().getAction();
        if (ACTION_STOP.equals(action)) {
            // Send an explicit cleanup action: the system's VPN binding can
            // keep the service alive after stopService(), preventing onDestroy.
            startService(new Intent(this, TProxyService.class)
                    .setAction(TProxyService.ACTION_STOP));
            finish();
            return;
        }

        if (!ACTION_AUTHORIZE.equals(action) && !ACTION_APPLY.equals(action)) {
            finish();
            return;
        }

        Intent permission = VpnService.prepare(this);
        if (permission == null) {
            continueAction();
        } else {
            startActivityForResult(permission, VPN_PERMISSION_REQUEST);
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != VPN_PERMISSION_REQUEST) {
            return;
        }
        if (resultCode == RESULT_OK) {
            continueAction();
        } else {
            Toast.makeText(this, "未获得 VPN 授权", Toast.LENGTH_SHORT).show();
            finish();
        }
    }

    private void continueAction() {
        if (ACTION_AUTHORIZE.equals(getIntent().getAction())) {
            Toast.makeText(this, "ADB Monster 弱网助手已授权", Toast.LENGTH_SHORT).show();
            finish();
            return;
        }

        Intent serviceIntent = new Intent(this, TProxyService.class)
                .setAction(TProxyService.ACTION_START)
                .putExtras(getIntent());
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(serviceIntent);
        } else {
            startService(serviceIntent);
        }
        finish();
    }
}
