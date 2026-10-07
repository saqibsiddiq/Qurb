package com.qurb

/**
 * This phone's copies of files it sent, kept after they arrived (decision
 * 0030). A phone has no storage cap to let go of them, so it kept every one;
 * the person lets go of them by name, from Settings or from Android's storage
 * screen, and is told first what that can cost.
 */
object SentCopies {
    const val COST = "These are this phone's copies of files it sent, which the devices you sent them to " +
        "have taken. If one of those devices has deleted its copy since, this is the last one, and " +
        "letting go of it loses that file. Files in your folders are not touched."
}
