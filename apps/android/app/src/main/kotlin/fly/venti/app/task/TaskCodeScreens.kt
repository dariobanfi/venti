package fly.venti.app.task

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.compose.LocalLifecycleOwner
import app.venti.design.VentiFlight
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.design.Common
import fly.venti.app.design.Group
import fly.venti.app.design.PillShape
import fly.venti.app.design.SectionFooter
import fly.venti.app.design.Venti
import fly.venti.app.settings.SwitchRow
import java.util.concurrent.Executors
import kotlin.math.floor
import kotlin.math.min

/**
 * What the task's code and its scanner say. The core has no words for
 * them: they are the iPhone's, with the phone called a phone.
 */
object TaskCodeWords {
    /** The entry of the task's menu, and the row among the ways to bring a task in. */
    const val SHOW = "Show as a QR code"
    const val SCAN = "Scan a QR code"

    /** The sheet with the code. */
    const val TITLE = "Task as a QR code"
    const val DESCRIBED = "The task as a QR code"
    const val NOT_DRAWN = "The code could not be drawn."
    const val COMPRESSED = "Compressed"
    const val COMPRESSED_NOTE =
        "A compressed code has fewer dots and is read more easily in the sun. An instrument with old " +
            "software may only read the other kind."

    /** The scanner. */
    const val SCAN_TITLE = "Scan a task"
    const val HOLD = "Hold the task's QR code in view."
    const val NO_CAMERA_TITLE = "The camera cannot be used"
    const val NO_CAMERA =
        "Allow Venti to use the camera in the phone's settings, or copy the task from a message and paste it."
    const val OPEN_SETTINGS = "Open the phone's settings"
}

/** The task as a QR code for another pilot's instrument. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskCodeSheet(tasks: TaskModel, onDismiss: () -> Unit) {
    // Compressed, the code has fewer dots and scans better in the sun.
    var isCompressed by rememberSaveable { mutableStateOf(true) }
    val dots = remember(tasks.task, isCompressed) { tasks.code(isCompressed)?.let(TaskCode::dots) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(start = VentiSpace.s5, end = VentiSpace.s2),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(TaskCodeWords.TITLE, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
            TextButton(onClick = onDismiss, modifier = Modifier.testTag("task-code-close")) { Text(Common.words.done) }
        }
        Column(
            Modifier
                .testTag("task-code-sheet")
                .verticalScroll(rememberScrollState())
                .padding(start = VentiSpace.s4, end = VentiSpace.s4, top = VentiSpace.s2, bottom = VentiSpace.s6),
        ) {
            if (dots != null) {
                CodePicture(dots, Modifier.fillMaxWidth().aspectRatio(1f).testTag("task-code"))
            } else {
                Text(
                    TaskCodeWords.NOT_DRAWN,
                    style = MaterialTheme.typography.bodyLarge,
                    color = Venti.colours.textSecondary,
                    modifier = Modifier.testTag("task-code-not-drawn"),
                )
            }
            Group(Modifier.padding(top = VentiSpace.s5)) {
                row {
                    SwitchRow(TaskCodeWords.COMPRESSED, isOn = isCompressed, modifier = Modifier.testTag("task-code-compressed")) {
                        isCompressed = it
                    }
                }
            }
            SectionFooter(TaskCodeWords.COMPRESSED_NOTE)
        }
    }
}

/**
 * The dots of a code, drawn. A code is read as dark on light, whatever the
 * appearance of the app: it takes the flight theme's white ground and
 * near-black ink in light and in dark.
 */
@Composable
private fun CodePicture(dots: TaskCode.Dots, modifier: Modifier = Modifier) {
    val ground = VentiFlight.bg
    val ink = VentiFlight.text
    Canvas(
        modifier
            .clip(MaterialTheme.shapes.medium)
            .background(ground)
            .semantics { contentDescription = TaskCodeWords.DESCRIBED },
    ) {
        // Whole pixels to a dot, so that no seam shows between two dark
        // ones, and the standard's empty room around the code.
        val dot = floor(min(size.width, size.height) / (dots.side + 2 * TaskCode.QUIET_DOTS))
        if (dot < 1f) return@Canvas
        val left = floor((size.width - dot * dots.side) / 2)
        val top = floor((size.height - dot * dots.side) / 2)
        for (row in 0 until dots.side) {
            var column = 0
            while (column < dots.side) {
                if (!dots.isDark(column, row)) {
                    column++
                    continue
                }
                // A run of dark dots is one rectangle.
                val start = column
                while (column < dots.side && dots.isDark(column, row)) column++
                drawRect(ink, Offset(left + start * dot, top + row * dot), Size((column - start) * dot, dot))
            }
        }
    }
}

/** Whether the app may use the camera, as far as the scanner knows. */
private enum class CameraAccess {
    /** Android is asking the pilot. */
    Asking,

    /** The pilot said no, the phone has no camera, or the camera did not start. */
    Refused,
    Allowed,
}

/**
 * Scans the QR code of a task with the camera: at a briefing the task is
 * on a screen or a sheet of paper, and typing it in is how turnpoints go
 * wrong. The camera is asked for when this screen opens, which is when
 * the pilot chose to scan, and never before.
 *
 * Not tried on a phone: the tests' emulator is shown this screen without
 * the permission only.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TaskScanScreen(tasks: TaskModel, onDismiss: () -> Unit) {
    val context = LocalContext.current
    var access by remember {
        mutableStateOf(if (mayUseCamera(context)) CameraAccess.Allowed else CameraAccess.Asking)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { allowed ->
        access = if (allowed) CameraAccess.Allowed else CameraAccess.Refused
    }
    LaunchedEffect(Unit) {
        if (access == CameraAccess.Asking) {
            if (context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)) {
                ask.launch(Manifest.permission.CAMERA)
            } else {
                access = CameraAccess.Refused
            }
        }
    }
    // Back from the phone's settings, where the pilot may have allowed it.
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) {
        if (access == CameraAccess.Refused && mayUseCamera(context)) access = CameraAccess.Allowed
    }
    // Why the code in view was not taken, in the core's words, and the
    // code they are about: the camera sees the same code many times a second.
    var problem by remember { mutableStateOf<String?>(null) }
    var refused by remember { mutableStateOf<String?>(null) }
    var isTaken by remember { mutableStateOf(false) }

    BackHandler(onBack = onDismiss)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(TaskCodeWords.SCAN_TITLE) },
                navigationIcon = {
                    IconButton(onClick = onDismiss) {
                        Icon(painterResource(R.drawable.ic_close), contentDescription = Common.words.cancel)
                    }
                },
            )
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).testTag("task-scan-screen")) {
            when (access) {
                // Android's question stands over the app.
                CameraAccess.Asking -> Unit
                CameraAccess.Refused -> NoCamera(onOpenSettings = { openAppSettings(context) })
                CameraAccess.Allowed -> {
                    CodeCamera(
                        onCode = { text ->
                            if (!isTaken && text != refused) {
                                // The core says whether the code holds a task.
                                if (tasks.keep(text)) {
                                    isTaken = true
                                    onDismiss()
                                } else {
                                    refused = text
                                    problem = tasks.problem
                                }
                            }
                        },
                        onUnavailable = { access = CameraAccess.Refused },
                        modifier = Modifier.fillMaxSize(),
                    )
                    Surface(
                        modifier = Modifier.align(Alignment.BottomCenter).padding(VentiSpace.s5).testTag("task-scan-words"),
                        shape = PillShape,
                        color = MaterialTheme.colorScheme.surface,
                        // The line a card of the pack has round it.
                        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                    ) {
                        Text(
                            problem ?: TaskCodeWords.HOLD,
                            style = MaterialTheme.typography.labelLarge,
                            textAlign = TextAlign.Center,
                            modifier = Modifier
                                .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s3)
                                .semantics { liveRegion = LiveRegionMode.Polite },
                        )
                    }
                }
            }
        }
    }
}

/** The scanner without a camera: what is the matter, and the two ways on. */
@Composable
private fun NoCamera(onOpenSettings: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("task-scan-no-camera")
            .padding(horizontal = VentiSpace.s4, vertical = VentiSpace.s7),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s2),
    ) {
        Icon(
            painterResource(R.drawable.ic_qr_code_scanner),
            contentDescription = null,
            modifier = Modifier.size(VentiSpace.s7),
            tint = Venti.colours.textSecondary,
        )
        Text(TaskCodeWords.NO_CAMERA_TITLE, style = MaterialTheme.typography.titleLarge, textAlign = TextAlign.Center)
        Text(
            TaskCodeWords.NO_CAMERA,
            style = MaterialTheme.typography.bodyLarge,
            color = Venti.colours.textSecondary,
            textAlign = TextAlign.Center,
        )
        OutlinedButton(
            onClick = onOpenSettings,
            modifier = Modifier.padding(top = VentiSpace.s3).testTag("task-scan-settings"),
        ) {
            Text(TaskCodeWords.OPEN_SETTINGS)
        }
    }
}

private fun mayUseCamera(context: Context): Boolean =
    ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

/** Venti's page in Android's settings, where a permission refused for good is given. */
private fun openAppSettings(context: Context) {
    val page = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.fromParts("package", context.packageName, null))
        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    runCatching { context.startActivity(page) }
}

/**
 * The camera, looking for QR codes: its picture on screen, and each frame
 * handed to the reader on a thread of its own. The text of a code in view
 * is handed over with every frame that shows it.
 */
@Composable
private fun CodeCamera(onCode: (String) -> Unit, onUnavailable: () -> Unit, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val found by rememberUpdatedState(onCode)
    val failed by rememberUpdatedState(onUnavailable)
    val picture = remember { PreviewView(context).apply { scaleType = PreviewView.ScaleType.FILL_CENTER } }

    DisposableEffect(owner) {
        val frames = Executors.newSingleThreadExecutor()
        val main = ContextCompat.getMainExecutor(context)
        val asked = ProcessCameraProvider.getInstance(context)
        var cameras: ProcessCameraProvider? = null
        var hasLeft = false
        asked.addListener(
            {
                if (hasLeft) return@addListener
                try {
                    val provider = asked.get()
                    cameras = provider
                    val shown = Preview.Builder().build().also { it.surfaceProvider = picture.surfaceProvider }
                    val read = ImageAnalysis.Builder()
                        // A frame that comes while one is being read is dropped.
                        .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                        .build()
                    read.setAnalyzer(frames) { frame ->
                        val text = frame.use(::codeIn)
                        if (text != null) main.execute { found(text) }
                    }
                    // The camera on the back, or the one there is.
                    val camera = if (provider.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA)) {
                        CameraSelector.DEFAULT_BACK_CAMERA
                    } else {
                        CameraSelector.DEFAULT_FRONT_CAMERA
                    }
                    provider.unbindAll()
                    provider.bindToLifecycle(owner, camera, shown, read)
                } catch (_: Exception) {
                    // No camera that can be opened: the screen says so.
                    failed()
                }
            },
            main,
        )
        onDispose {
            hasLeft = true
            cameras?.unbindAll()
            frames.shutdown()
        }
    }

    AndroidView(factory = { picture }, modifier = modifier)
}

/** The text of the QR code in a frame of the camera, read from its plane of brightness. */
private fun codeIn(frame: ImageProxy): String? {
    val plane = frame.planes.firstOrNull() ?: return null
    val brightness = ByteArray(plane.buffer.remaining())
    plane.buffer.get(brightness)
    return TaskCode.read(brightness, plane.rowStride, frame.width, frame.height)
}
