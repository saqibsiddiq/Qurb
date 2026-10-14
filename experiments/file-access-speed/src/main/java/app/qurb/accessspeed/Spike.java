package app.qurb.accessspeed;

import android.app.Activity;
import android.content.Intent;
import android.content.SharedPreferences;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.Environment;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract;
import android.provider.Settings;
import android.util.Log;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

/**
 * How much faster is reading a folder with "All files access" than through a
 * folder the person picked? Throwaway: see README.md.
 *
 * Both ways list the same folder, then read every byte of the same kind of
 * files, four at a time as Qurb does: the picked folder through the system's
 * document provider (what Qurb does for WhatsApp now), and directly by path.
 * The files are split into two interleaved halves, one for each way, so that
 * neither warms the cache for the other and both get files of the same sizes.
 */
public class Spike extends Activity {
    private static final String TAG = "access-speed";
    private static final String DEFAULT = "primary:Android/media/com.whatsapp/WhatsApp";
    private static final int PICK = 1;
    private static final int THREADS = 4;

    private TextView out;
    private SharedPreferences prefs;
    /** Which half goes which way: run once each, so that neither way always gets the larger files. */
    private boolean swap;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        prefs = getSharedPreferences("spike", MODE_PRIVATE);

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setPadding(32, 140, 32, 32);
        column.addView(button("Pick a folder", v -> pick()));
        column.addView(button("All files access", v -> startActivity(
                new Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                        Uri.parse("package:" + getPackageName())))));
        column.addView(button("Measure", v -> measureInBackground(2000)));
        out = new TextView(this);
        out.setTextIsSelectable(true);
        column.addView(out);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(column);
        setContentView(scroll);

        say("All files access: " + Environment.isExternalStorageManager());
        say("Picked: " + prefs.getString("tree", "nothing yet"));

        // `am start -n app.qurb.accessspeed/.Spike --es run measure --ei files 2000 [--ez swap true]`
        if ("measure".equals(getIntent().getStringExtra("run"))) {
            swap = getIntent().getBooleanExtra("swap", false);
            measureInBackground(getIntent().getIntExtra("files", 2000));
        }
    }

    private Button button(String label, android.view.View.OnClickListener tap) {
        Button b = new Button(this);
        b.setText(label);
        b.setOnClickListener(tap);
        return b;
    }

    private void pick() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.putExtra(DocumentsContract.EXTRA_INITIAL_URI,
                DocumentsContract.buildDocumentUri("com.android.externalstorage.documents", DEFAULT));
        startActivityForResult(intent, PICK);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        if (request != PICK || result != RESULT_OK || data == null) return;
        Uri tree = data.getData();
        getContentResolver().takePersistableUriPermission(tree, Intent.FLAG_GRANT_READ_URI_PERMISSION);
        prefs.edit().putString("tree", tree.toString()).apply();
        say("Picked: " + tree);
    }

    private void measureInBackground(int files) {
        new Thread(() -> {
            try {
                measure(files);
            } catch (Exception e) {
                say("failed: " + e);
                Log.e(TAG, "failed", e);
            }
        }).start();
    }

    /** One file found by listing: where it is both ways, and its size. */
    private static final class Found {
        final String id;
        final File file;
        final long size;
        Found(String id, File file, long size) { this.id = id; this.file = file; this.size = size; }
    }

    private void measure(int wanted) throws Exception {
        String saved = prefs.getString("tree", null);
        if (saved == null) { say("Pick a folder first."); return; }
        Uri tree = Uri.parse(saved);
        String root = DocumentsContract.getTreeDocumentId(tree);
        File base = new File(Environment.getExternalStorageDirectory(), root.substring(root.indexOf(':') + 1));
        say("All files access: " + Environment.isExternalStorageManager() + ", folder " + base);

        // Listing, both ways. Names starting with a dot are skipped, as Qurb does.
        long t = System.nanoTime();
        int[] counted = {0};
        listPicked(tree, root, counted);
        double pickedList = ms(t);
        say(String.format("listed through the picked folder: %d files in %.0f ms", counted[0], pickedList));

        t = System.nanoTime();
        List<Found> all = new ArrayList<>();
        listDirect(base, root, all);
        double directList = ms(t);
        say(String.format("listed directly: %d files in %.0f ms", all.size(), directList));
        if (all.isEmpty()) { say("Nothing to read directly: is All files access on?"); return; }

        // Two interleaved halves of the same sample.
        List<Found> sample = all.subList(0, Math.min(wanted, all.size()));
        List<Found> viaPicked = new ArrayList<>(), viaPath = new ArrayList<>();
        for (int i = 0; i < sample.size(); i++) ((i % 2 == 0) != swap ? viaPicked : viaPath).add(sample.get(i));

        report("through the picked folder", viaPicked, f -> {
            Uri doc = DocumentsContract.buildDocumentUriUsingTree(tree, f.id);
            try (ParcelFileDescriptor pfd = getContentResolver().openFileDescriptor(doc, "r");
                 InputStream in = new FileInputStream(pfd.getFileDescriptor())) {
                return drain(in);
            }
        });
        report("directly by path", viaPath, f -> {
            try (InputStream in = new FileInputStream(f.file)) {
                return drain(in);
            }
        });
    }

    private void listPicked(Uri tree, String id, int[] counted) {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id);
        String[] columns = {
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
        };
        try (Cursor c = getContentResolver().query(children, columns, null, null, null)) {
            while (c != null && c.moveToNext()) {
                String child = c.getString(0), name = c.getString(1), mime = c.getString(2);
                if (name.startsWith(".")) continue;
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mime)) listPicked(tree, child, counted);
                else counted[0]++;
            }
        }
    }

    private void listDirect(File dir, String id, List<Found> into) {
        File[] entries = dir.listFiles();
        if (entries == null) return;
        for (File entry : entries) {
            if (entry.getName().startsWith(".")) continue;
            String child = id + "/" + entry.getName();
            if (entry.isDirectory()) listDirect(entry, child, into);
            else into.add(new Found(child, entry, entry.length()));
        }
    }

    private interface Reader { long read(Found f) throws Exception; }

    private void report(String how, List<Found> files, Reader reader) throws Exception {
        AtomicLong bytes = new AtomicLong();
        AtomicLong failed = new AtomicLong();
        ExecutorService pool = Executors.newFixedThreadPool(THREADS);
        long t = System.nanoTime();
        for (Found f : files) {
            pool.execute(() -> {
                try {
                    bytes.addAndGet(reader.read(f));
                } catch (Exception e) {
                    failed.incrementAndGet();
                }
            });
        }
        pool.shutdown();
        pool.awaitTermination(1, TimeUnit.HOURS);
        double took = ms(t);
        say(String.format("read %s: %d files, %.1f MB in %.0f ms: %.2f ms a file, %.1f MB/s, %d failed",
                how, files.size(), bytes.get() / 1e6, took, took / files.size(),
                bytes.get() / 1e6 / (took / 1000), failed.get()));
    }

    private static long drain(InputStream in) throws Exception {
        byte[] buffer = new byte[1 << 16];
        long total = 0;
        for (int n; (n = in.read(buffer)) > 0; ) total += n;
        return total;
    }

    private static double ms(long since) {
        return (System.nanoTime() - since) / 1e6;
    }

    private void say(String line) {
        Log.i(TAG, line);
        runOnUiThread(() -> out.append(line + "\n"));
    }
}
