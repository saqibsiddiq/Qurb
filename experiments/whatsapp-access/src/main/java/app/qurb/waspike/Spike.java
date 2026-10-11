package app.qurb.waspike;

import android.app.Activity;
import android.content.ContentResolver;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.UriPermission;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.SystemClock;
import android.provider.DocumentsContract;
import android.provider.DocumentsContract.Document;
import android.util.Log;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.InputStream;
import java.io.OutputStream;
import java.security.MessageDigest;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;

/**
 * Throwaway (see README.md). Can an app with no storage permission reach
 * WhatsApp's folder through a folder the person picks once -- list it, read
 * its backups, write into it -- and how long does walking it take?
 *
 * Everything it finds goes to the screen and to logcat under "wa-spike", which
 * is how the laptop reads it.
 */
public class Spike extends Activity {

    private static final String TAG = "wa-spike";
    private static final int PICK = 1;
    private static final String STORAGE = "com.android.externalstorage.documents";
    private static final String[] COLUMNS = {
        Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE,
        Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED,
    };

    private TextView out;
    private SharedPreferences prefs;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved);
        prefs = getSharedPreferences("spike", MODE_PRIVATE);

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        // Android 15 draws edge to edge: keep clear of the bars, or the first
        // button sits under them and a tap never reaches it (it did, on the
        // S23).
        column.setOnApplyWindowInsetsListener((v, insets) -> {
            android.graphics.Insets bars = insets.getInsets(android.view.WindowInsets.Type.systemBars());
            v.setPadding(32, bars.top + 32, 32, bars.bottom + 32);
            return insets;
        });
        column.addView(button("1. Pick WhatsApp's folder", () -> pick("primary:Android/media/com.whatsapp/WhatsApp")));
        column.addView(button("2. Read it", () -> work(this::read)));
        column.addView(button("3. Write test (a file, then deleted)", () -> work(this::writeTest)));
        column.addView(button("4. Pick Android/media (a new phone)", () -> pick("primary:Android/media")));
        column.addView(button("5. Make com.whatsapp/WhatsApp/Databases there", () -> work(this::makeFolders)));
        out = new TextView(this);
        out.setTextIsSelectable(true);
        out.setTextSize(12);
        ScrollView scroll = new ScrollView(this);
        scroll.addView(out);
        column.addView(scroll);
        setContentView(column);

        // What survived from before: a grant that outlives a restart is the
        // whole point, since a backup runs with nobody there to pick again.
        for (UriPermission p : getContentResolver().getPersistedUriPermissions()) {
            say("held: " + p.getUri() + " read=" + p.isReadPermission() + " write=" + p.isWritePermission());
        }
        say("picked: " + prefs.getString("tree", "(none)"));
    }

    private Button button(String label, Runnable act) {
        Button b = new Button(this);
        b.setText(label);
        b.setAllCaps(false);
        b.setOnClickListener(v -> act.run());
        return b;
    }

    private void pick(String startAt) {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.putExtra(DocumentsContract.EXTRA_INITIAL_URI, DocumentsContract.buildDocumentUri(STORAGE, startAt));
        intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
            | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        startActivityForResult(intent, PICK);
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != PICK) return;
        if (result != RESULT_OK || data == null || data.getData() == null) {
            say("pick: refused or cancelled (result " + result + ")");
            return;
        }
        Uri tree = data.getData();
        try {
            getContentResolver().takePersistableUriPermission(tree,
                Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            say("pick: " + tree + " (kept)");
        } catch (SecurityException e) {
            say("pick: " + tree + " but could not keep it: " + e.getMessage());
        }
        prefs.edit().putString("tree", tree.toString()).apply();
    }

    private interface Job { void run(Uri tree) throws Exception; }

    private void work(Job job) {
        String picked = prefs.getString("tree", null);
        if (picked == null) { say("pick a folder first"); return; }
        Uri tree = Uri.parse(picked);
        new Thread(() -> {
            try {
                job.run(tree);
            } catch (Exception e) {
                say("failed: " + e);
            }
        }).start();
    }

    // ------------------------------------------------------------------ read

    private static final class Tally { long files, bytes; }

    private String newestBackupId;
    private String newestBackupName;
    private long newestBackupAt = -1;

    /** Walk the whole tree: every file counted under its top folder, the
     *  backups listed by name, and the newest one read through. */
    private void read(Uri tree) throws Exception {
        newestBackupId = null;
        newestBackupAt = -1;
        Map<String, Tally> byTop = new LinkedHashMap<>();
        long started = SystemClock.elapsedRealtime();
        int[] queries = {0};
        walk(tree, DocumentsContract.getTreeDocumentId(tree), "", byTop, queries);
        long took = SystemClock.elapsedRealtime() - started;

        long files = 0, bytes = 0;
        for (Map.Entry<String, Tally> e : byTop.entrySet()) {
            say(String.format(Locale.ROOT, "top %-28s %7d files %10s", e.getKey(), e.getValue().files, size(e.getValue().bytes)));
            files += e.getValue().files;
            bytes += e.getValue().bytes;
        }
        say(String.format(Locale.ROOT, "walk: %d files, %s, %d folder queries, in %d ms", files, size(bytes), queries[0], took));

        if (newestBackupId == null) { say("no chat backup found"); return; }
        Uri doc = DocumentsContract.buildDocumentUriUsingTree(tree, newestBackupId);
        MessageDigest sha = MessageDigest.getInstance("SHA-256");
        byte[] buffer = new byte[1 << 16];
        long read = 0;
        started = SystemClock.elapsedRealtime();
        try (InputStream in = getContentResolver().openInputStream(doc)) {
            for (int n; (n = in.read(buffer)) > 0; ) { sha.update(buffer, 0, n); read += n; }
        }
        took = Math.max(1, SystemClock.elapsedRealtime() - started);
        say(String.format(Locale.ROOT, "read %s: %s in %d ms (%.1f MB/s), sha256 %s",
            newestBackupName, size(read), took, read / 1e3 / took, hex(sha.digest()).substring(0, 16)));
    }

    private void walk(Uri tree, String id, String path, Map<String, Tally> byTop, int[] queries) {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, id);
        queries[0]++;
        try (Cursor c = getContentResolver().query(children, COLUMNS, null, null, null)) {
            if (c == null) return;
            while (c.moveToNext()) {
                String child = c.getString(0), name = c.getString(1), mime = c.getString(2);
                long length = c.isNull(3) ? 0 : c.getLong(3), at = c.isNull(4) ? 0 : c.getLong(4);
                String here = path.isEmpty() ? name : path + "/" + name;
                if (Document.MIME_TYPE_DIR.equals(mime)) {
                    walk(tree, child, here, byTop, queries);
                    continue;
                }
                // Grouped two deep under Media (Media/WhatsApp Images, ...),
                // one deep elsewhere (Databases, Backups).
                String[] parts = here.split("/");
                String top = parts.length > 2 && parts[0].equals("Media") ? parts[0] + "/" + parts[1] : parts.length > 1 ? parts[0] : "(top)";
                Tally t = byTop.computeIfAbsent(top, k -> new Tally());
                t.files++;
                t.bytes += length;
                if (parts[0].equals("Databases") || parts[0].equals("Backups")) {
                    say(String.format(Locale.ROOT, "  %s  %s  %tF %<tR", here, size(length), at));
                }
                if (name.startsWith("msgstore") && at > newestBackupAt) {
                    newestBackupAt = at;
                    newestBackupId = child;
                    newestBackupName = here;
                }
            }
        }
    }

    // ----------------------------------------------------------------- write

    /** What restoring needs: put a file into the picked folder and read it
     *  back. Deleted again whatever happens. */
    private void writeTest(Uri tree) throws Exception {
        ContentResolver r = getContentResolver();
        Uri root = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree));
        Uri made = DocumentsContract.createDocument(r, root, "application/octet-stream", "qurb-write-test.bin");
        if (made == null) { say("write: could not create a file"); return; }
        try {
            byte[] bytes = "qurb was here".getBytes("UTF-8");
            try (OutputStream o = r.openOutputStream(made)) { o.write(bytes); }
            byte[] back = new byte[64];
            int n;
            try (InputStream in = r.openInputStream(made)) { n = in.read(back); }
            say("write: made " + made.getLastPathSegment() + ", read back " + n + " bytes, same=" + (n == bytes.length));
        } finally {
            say("write: deleted again=" + DocumentsContract.deleteDocument(r, made));
        }
    }

    /** On a phone WhatsApp has not run on yet, its folder may not exist:
     *  can the app make it, so a backup can be put there before restoring? */
    private void makeFolders(Uri tree) throws Exception {
        ContentResolver r = getContentResolver();
        Uri at = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree));
        for (String name : new String[] {"com.whatsapp", "WhatsApp", "Databases"}) {
            Uri existing = child(tree, at, name);
            at = existing != null ? existing : DocumentsContract.createDocument(r, at, Document.MIME_TYPE_DIR, name);
            say("folder " + name + ": " + (existing != null ? "there already" : at == null ? "could not make" : "made"));
            if (at == null) return;
        }
    }

    private Uri child(Uri tree, Uri parent, String name) {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getDocumentId(parent));
        try (Cursor c = getContentResolver().query(children, COLUMNS, null, null, null)) {
            while (c != null && c.moveToNext()) {
                if (name.equals(c.getString(1))) return DocumentsContract.buildDocumentUriUsingTree(tree, c.getString(0));
            }
        }
        return null;
    }

    // ------------------------------------------------------------------ misc

    private void say(String line) {
        Log.i(TAG, line);
        runOnUiThread(() -> out.append(line + "\n"));
    }

    private static String size(long bytes) {
        if (bytes >= 1L << 30) return String.format(Locale.ROOT, "%.2f GB", bytes / (double) (1L << 30));
        if (bytes >= 1L << 20) return String.format(Locale.ROOT, "%.1f MB", bytes / (double) (1L << 20));
        if (bytes >= 1L << 10) return String.format(Locale.ROOT, "%.1f KB", bytes / 1024.0);
        return bytes + " B";
    }

    private static String hex(byte[] b) {
        StringBuilder s = new StringBuilder();
        for (byte x : b) s.append(String.format("%02x", x));
        return s.toString();
    }
}
