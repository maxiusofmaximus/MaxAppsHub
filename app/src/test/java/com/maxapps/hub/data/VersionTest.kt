package com.maxapps.hub.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class VersionTest {

    @Test
    fun `prerelease is older than its stable release`() {
        assertTrue(isNewerVersion("1.0.5", "1.0.5-alpha.1"))
        assertFalse(isNewerVersion("1.0.5-alpha.1", "1.0.5"))
    }

    @Test
    fun `numeric prerelease identifiers compare by value`() {
        assertTrue(isNewerVersion("1.0.5-alpha.2", "1.0.5-alpha.1"))
        assertTrue(isNewerVersion("1.0.5-alpha.10", "1.0.5-alpha.9"))
    }

    @Test
    fun `leading v and build metadata are ignored`() {
        assertEquals(0, Version.parse("v1.0.5+17")!!.compareTo(Version.parse("1.0.5")!!))
        assertFalse(isNewerVersion("v1.0.5+17", "1.0.5"))
    }

    @Test
    fun `core components compare numerically`() {
        assertTrue(isNewerVersion("1.0.10", "1.0.9"))
    }

    @Test
    fun `alpha of a newer core beats stable of an older one`() {
        assertTrue(isNewerVersion("1.0.6-alpha.1", "1.0.5"))
    }

    @Test
    fun `alphanumeric identifiers sort after numeric ones and a longer set wins`() {
        assertTrue(isNewerVersion("1.0.0-beta", "1.0.0-1"))
        assertTrue(isNewerVersion("1.0.0-alpha.1", "1.0.0-alpha"))
    }

    @Test
    fun `unparseable versions are never newer`() {
        assertNull(Version.parse("nightly"))
        assertNull(Version.parse("1.0.5-"))
        assertFalse(isNewerVersion("nightly", "1.0.0"))
        assertFalse(isNewerVersion("1.0.1", "nightly"))
    }
}
