// Loads a bench page in WebKitGTK (webkit2gtk-4.1, the library Tauri uses on
// Linux), in a GTK window. run.mjs drives it as it drives the WKWebView
// runner:
//
//   wkgtk-runner <url> <width> <height>
//
// stdout: "snapped <path>" after a screenshot.
// stdin:  "snap <path>" saves a PNG of the visible page.
//
// Build: gcc -O2 -o wkgtk-runner runner.c $(pkg-config --cflags --libs webkit2gtk-4.1)

#include <gtk/gtk.h>
#include <stdio.h>
#include <stdlib.h>
#include <webkit2/webkit2.h>

static WebKitWebView *view;

static void on_snapshot(GObject *source, GAsyncResult *result, gpointer data) {
  char *path = data;
  GError *error = NULL;
  cairo_surface_t *surface = webkit_web_view_get_snapshot_finish(WEBKIT_WEB_VIEW(source), result, &error);
  if (surface) {
    cairo_surface_write_to_png(surface, path);
    cairo_surface_destroy(surface);
  } else if (error) {
    fprintf(stderr, "snapshot failed: %s\n", error->message);
    g_error_free(error);
  }
  printf("snapped %s\n", path);
  fflush(stdout);
  g_free(path);
}

static gboolean on_stdin(GIOChannel *channel, GIOCondition condition, gpointer data) {
  gchar *line = NULL;
  gsize length = 0;
  GIOStatus status = g_io_channel_read_line(channel, &line, &length, NULL, NULL);
  if (status == G_IO_STATUS_EOF || status == G_IO_STATUS_ERROR) {
    exit(0);
  }
  if (line && g_str_has_prefix(line, "snap ")) {
    g_strchomp(line);
    webkit_web_view_get_snapshot(view, WEBKIT_SNAPSHOT_REGION_VISIBLE, WEBKIT_SNAPSHOT_OPTIONS_NONE, NULL,
                                 on_snapshot, g_strdup(line + 5));
  }
  g_free(line);
  return TRUE;
}

static void on_terminated(WebKitWebView *web_view, WebKitWebProcessTerminationReason reason, gpointer data) {
  fprintf(stderr, "web process terminated: %d\n", reason);
  exit(3);
}

int main(int argc, char **argv) {
  gtk_init(&argc, &argv);
  if (argc < 4) {
    fprintf(stderr, "usage: wkgtk-runner <url> <width> <height>\n");
    return 2;
  }
  GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
  gtk_window_set_default_size(GTK_WINDOW(window), atoi(argv[2]), atoi(argv[3]));
  gtk_window_set_decorated(GTK_WINDOW(window), FALSE);
  view = WEBKIT_WEB_VIEW(webkit_web_view_new());
  WebKitSettings *settings = webkit_web_view_get_settings(view);
  webkit_settings_set_enable_webgl(settings, TRUE);
  webkit_settings_set_enable_developer_extras(settings, TRUE);
  fprintf(stderr, "hardware acceleration policy: %d\n", webkit_settings_get_hardware_acceleration_policy(settings));
  g_signal_connect(view, "web-process-terminated", G_CALLBACK(on_terminated), NULL);
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));
  webkit_web_view_load_uri(view, argv[1]);
  gtk_widget_show_all(window);
  GIOChannel *in = g_io_channel_unix_new(0);
  g_io_add_watch(in, G_IO_IN | G_IO_HUP, on_stdin, NULL);
  gtk_main();
  return 0;
}
