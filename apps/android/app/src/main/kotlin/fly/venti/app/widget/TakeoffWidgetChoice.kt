package fly.venti.app.widget

import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.lifecycleScope
import app.venti.design.VentiSize
import app.venti.design.VentiSpace
import fly.venti.app.R
import fly.venti.app.Session
import fly.venti.app.VentiApplication
import fly.venti.app.design.Common
import fly.venti.app.design.Venti
import fly.venti.app.design.VentiTheme
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The widget's own screen, which the launcher opens to set it up: the
 * pilot's own take-offs, of which the widget shows the one tapped. Until
 * one is chosen it shows the first.
 */
class TakeoffWidgetChoice : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
        // Left without a choice, the launcher is told that nothing was set up.
        setResult(RESULT_CANCELED, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId))
        if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
            finish()
            return
        }
        val store = TakeoffWidgets.store(this)
        setContent {
            VentiTheme(dark = isSystemInDarkTheme()) {
                val takeoffs by produceState<List<WidgetTakeoff>?>(null) {
                    value = withContext(Dispatchers.IO) {
                        val session = (application as VentiApplication).session as? Session.Ready
                        session?.let { TakeoffWidgets.takeoffs(it.keptSites) }.orEmpty()
                    }
                }
                Choice(
                    takeoffs = takeoffs,
                    shown = takeoffs?.let { WidgetRules.takeoff(store.chosen(widgetId), it) },
                    onChoose = { takeoff ->
                        store.choose(widgetId, takeoff.id)
                        done(widgetId)
                    },
                    onDone = { done(widgetId) },
                )
            }
        }
    }

    /** The widget is drawn with what was chosen, its forecast is read, and the launcher is told. */
    private fun done(widgetId: Int) {
        setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId))
        TakeoffWidgetRefresh.schedule(applicationContext)
        TakeoffWidgetRefresh.now(applicationContext)
        lifecycleScope.launch {
            TakeoffWidgets.redraw(applicationContext)
            finish()
        }
    }
}

/** The list to choose from; without a take-off, the sentence the widget itself then shows. */
@Composable
private fun Choice(
    /** `null` while they are being read. */
    takeoffs: List<WidgetTakeoff>?,
    shown: WidgetTakeoff?,
    onChoose: (WidgetTakeoff) -> Unit,
    onDone: () -> Unit,
) {
    val colours = Venti.colours
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(colours.bg)
            .safeDrawingPadding()
            .padding(VentiSpace.s4)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(VentiSpace.s3),
    ) {
        Text(
            stringResource(R.string.takeoff_widget_choose),
            style = MaterialTheme.typography.headlineMedium,
            color = colours.text,
            modifier = Modifier.semantics { heading() },
        )
        if (takeoffs == null) return@Column
        if (takeoffs.isEmpty()) {
            Text(stringResource(R.string.takeoff_widget_no_takeoff), color = colours.textSecondary)
        }
        Column {
            takeoffs.forEachIndexed { index, takeoff ->
                if (index > 0) HorizontalDivider(color = colours.border)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("widget-takeoff")
                        .heightIn(min = VentiSize.touch)
                        .clickable { onChoose(takeoff) }
                        .semantics { selected = takeoff == shown }
                        .padding(vertical = VentiSpace.s3),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(VentiSpace.s3),
                ) {
                    Icon(
                        painterResource(R.drawable.venti_site),
                        contentDescription = null,
                        tint = colours.text,
                        modifier = Modifier.size(VentiSize.icon),
                    )
                    Text(takeoff.name, style = MaterialTheme.typography.titleLarge, color = colours.text, modifier = Modifier.weight(1f))
                    if (takeoff == shown) {
                        Icon(
                            painterResource(R.drawable.ic_check),
                            contentDescription = null,
                            tint = colours.text,
                            modifier = Modifier.size(VentiSize.icon),
                        )
                    }
                }
            }
        }
        OutlinedButton(
            onClick = onDone,
            modifier = Modifier.fillMaxWidth().heightIn(min = VentiSize.touch).testTag("widget-choice-done"),
        ) {
            Text(Common.words.done)
        }
    }
}
