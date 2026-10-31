package dev.utilbox.app

import android.app.Activity
import android.content.ContentValues
import android.content.Intent
import android.net.Uri
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.util.UUID

@InvokeArg
class PickArgs {
  var multiple: Boolean = false
  var mimeTypes: Array<String> = arrayOf("*/*")
}

@InvokeArg
class ExportArgs {
  lateinit var path: String
}

@InvokeArg
class OpenArgs {
  lateinit var key: String
}

/**
 * Bridges the storage access framework to the Rust engine.
 *
 * Inputs are copied from the system picker into the app cache so the engine
 * (FFmpeg) can read plain paths. Outputs are published to Download/UtilBox
 * through MediaStore, which needs no storage permission.
 */
@TauriPlugin
class UtilPlugin(private val activity: Activity) : Plugin(activity) {
  private val prefs by lazy {
    activity.getSharedPreferences("utilbox_exports", Activity.MODE_PRIVATE)
  }

  @Command
  fun pickFiles(invoke: Invoke) {
    val args = invoke.parseArgs(PickArgs::class.java)
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
      addCategory(Intent.CATEGORY_OPENABLE)
      type = "*/*"
      putExtra(Intent.EXTRA_MIME_TYPES, args.mimeTypes)
      putExtra(Intent.EXTRA_ALLOW_MULTIPLE, args.multiple)
    }
    startActivityForResult(invoke, intent, "onFilesPicked")
  }

  @ActivityCallback
  fun onFilesPicked(invoke: Invoke, result: ActivityResult) {
    val data = result.data
    if (result.resultCode != Activity.RESULT_OK || data == null) {
      invoke.resolve(JSObject().put("files", JSArray()))
      return
    }
    val uris = mutableListOf<Uri>()
    data.data?.let { uris.add(it) }
    data.clipData?.let { clip ->
      for (i in 0 until clip.itemCount) uris.add(clip.getItemAt(i).uri)
    }
    // Copying a large video takes a while; keep the UI thread free.
    Thread {
      try {
        val files = JSArray()
        for (uri in uris.distinct()) files.put(importUri(uri))
        invoke.resolve(JSObject().put("files", files))
      } catch (e: Exception) {
        invoke.reject("ファイルを読み込めません: ${e.message}")
      }
    }.start()
  }

  private fun importUri(uri: Uri): JSObject {
    val resolver = activity.contentResolver
    var name = "file"
    resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
      if (c.moveToFirst()) c.getString(0)?.let { name = it }
    }
    name = sanitize(name)
    if (!name.contains('.')) {
      val ext = resolver.getType(uri)?.let { MimeTypeMap.getSingleton().getExtensionFromMimeType(it) }
      if (ext != null) name = "$name.$ext"
    }
    val dir = File(activity.cacheDir, "imports/${UUID.randomUUID()}").apply { mkdirs() }
    val target = File(dir, name)
    resolver.openInputStream(uri).use { input ->
      requireNotNull(input) { "開けません" }
      target.outputStream().use { output -> input.copyTo(output, 1 shl 20) }
    }
    return JSObject()
      .put("path", target.absolutePath)
      .put("name", name)
      .put("size", target.length())
  }

  @Command
  fun exportFile(invoke: Invoke) {
    val args = invoke.parseArgs(ExportArgs::class.java)
    Thread {
      try {
        invoke.resolve(publish(File(args.path)))
      } catch (e: Exception) {
        invoke.reject("保存できません: ${e.message}")
      }
    }.start()
  }

  private fun publish(file: File): JSObject {
    require(file.isFile) { "出力ファイルが見つかりません" }
    val resolver = activity.contentResolver
    val ext = file.extension.lowercase()
    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
    val values = ContentValues().apply {
      put(MediaStore.MediaColumns.DISPLAY_NAME, file.name)
      put(MediaStore.MediaColumns.MIME_TYPE, mime)
      put(MediaStore.MediaColumns.RELATIVE_PATH, "$DOWNLOAD_DIR/$APP_DIR")
      put(MediaStore.MediaColumns.IS_PENDING, 1)
    }
    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
      ?: error("MediaStore に登録できません")
    try {
      resolver.openOutputStream(uri).use { out ->
        requireNotNull(out) { "書き込み先を開けません" }
        file.inputStream().use { it.copyTo(out, 1 shl 20) }
      }
      resolver.update(
        uri,
        ContentValues().apply { put(MediaStore.MediaColumns.IS_PENDING, 0) },
        null,
        null,
      )
    } catch (e: Exception) {
      resolver.delete(uri, null, null)
      throw e
    }
    // MediaStore renames on collisions, so read the final name back.
    var finalName = file.name
    resolver.query(uri, arrayOf(MediaStore.MediaColumns.DISPLAY_NAME), null, null, null)?.use { c ->
      if (c.moveToFirst()) c.getString(0)?.let { finalName = it }
    }
    val display = "$DOWNLOAD_DIR/$APP_DIR/$finalName"
    prefs.edit().putString(display, uri.toString()).apply()
    return JSObject().put("uri", uri.toString()).put("displayPath", display)
  }

  @Command
  fun openFile(invoke: Invoke) {
    val args = invoke.parseArgs(OpenArgs::class.java)
    val uriText = prefs.getString(args.key, null)
    if (uriText == null) {
      invoke.reject("このファイルは開けません。ファイルアプリの Download/$APP_DIR を確認してください")
      return
    }
    val uri = Uri.parse(uriText)
    val intent = Intent(Intent.ACTION_VIEW).apply {
      setDataAndType(uri, activity.contentResolver.getType(uri) ?: "*/*")
      addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    }
    try {
      activity.startActivity(intent)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject("開けるアプリが見つかりません")
    }
  }

  private fun sanitize(name: String): String {
    val cleaned = name.replace(Regex("[\\\\/:*?\"<>|\\u0000-\\u001f]"), "_").trim().trimStart('.')
    return cleaned.ifEmpty { "file" }.take(120)
  }

  companion object {
    private const val DOWNLOAD_DIR = "Download"
    private const val APP_DIR = "UtilBox"
  }
}
