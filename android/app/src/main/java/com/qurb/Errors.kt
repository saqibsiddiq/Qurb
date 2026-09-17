package com.qurb

import uniffi.qurb_mobile.QurbException

/**
 * Turn an engine error into something worth putting on a screen.
 *
 * Two reasons this exists rather than using `e.message`.
 *
 * **UniFFI's generated message is `"detail=${detail}"`.** The field name leaks
 * into every dialog, so a real failure reads `detail=connection lost: timed
 * out` — which looks like a bug in the app even when the app is fine.
 *
 * **The detail alone rarely says what to do.** "Connection lost: timed out" is
 * accurate and useless: the overwhelmingly likely cause on a home network is a
 * firewall dropping UDP, and nothing in the message hints at that. Each case
 * here pairs what happened with the first thing worth trying.
 */
fun QurbException.readable(): String = when (this) {
    is QurbException.Network -> network(detail)

    is QurbException.BadCode ->
        "That pairing code was not accepted.\n\n" +
            "Codes last five minutes and work once. Run `qurb pair` again on the " +
            "other device for a fresh one."

    is QurbException.BadPhrase ->
        "That recovery phrase was not accepted.\n\n" +
            "It is 24 words in order, separated by spaces. The phrase carries a " +
            "checksum, so a single mistyped or swapped word is caught here rather " +
            "than producing a key that opens nothing."

    is QurbException.Locked ->
        "The key could not be unlocked.\n\n$detail"

    is QurbException.NotSetUp ->
        "This device is not set up yet."

    is QurbException.NotFound ->
        "That file is not here any more."

    is QurbException.Storage ->
        "The store could not be read or written.\n\n$detail"

    is QurbException.Other -> detail
}

/**
 * Network failures, which are the ones a person can usually act on.
 *
 * A timeout on a home network almost always means something dropped the UDP
 * packets rather than that the other device is missing — it is worth naming the
 * firewall explicitly, because the alternative is someone concluding the app is
 * broken.
 */
private fun network(detail: String): String {
    val lower = detail.lowercase()
    return when {
        lower.contains("timed out") || lower.contains("timeout") ->
            "Could not reach the other device: it did not answer.\n\n" +
                "Both devices have to be awake and running at the same moment. " +
                "If they are, a firewall is the usual cause — sync uses UDP, and " +
                "a rule that allows ping will still drop it.\n\n" +
                "On Linux: sudo ufw allow from 192.168.0.0/16 to any proto udp"

        lower.contains("refused") ->
            "The other device refused the connection.\n\n" +
                "Check it is still running, and that the address in the pairing " +
                "code is the one it actually has now."

        lower.contains("signalling") || lower.contains("rendezvous") ->
            "Could not reach the rendezvous service.\n\n" +
                "Check the address under Rendezvous service, and that `qurb signal` " +
                "is running on that machine. From a phone it must be the computer's " +
                "address on your network, not localhost."

        lower.contains("dns") || lower.contains("resolve") ->
            "That address could not be looked up.\n\n$detail"

        else -> "Network problem.\n\n$detail"
    }
}
