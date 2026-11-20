package fly.venti.app.maps

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities

/** Whether the phone has a network that reaches the internet, as Android says. */
object Connectivity {
    /** Calls `changed` whenever the phone's default network comes or goes. */
    fun watch(context: Context, changed: (Boolean) -> Unit) {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return
        runCatching {
            manager.registerDefaultNetworkCallback(
                object : ConnectivityManager.NetworkCallback() {
                    override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
                        changed(capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET))
                    }

                    override fun onLost(network: Network) = changed(false)
                },
            )
        }
    }
}
