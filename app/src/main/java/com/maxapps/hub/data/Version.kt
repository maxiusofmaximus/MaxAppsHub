package com.maxapps.hub.data

import java.math.BigInteger

/**
 * Versión semver 2.0: `major.minor.patch[-prerelease]`.
 * Acepta una `v` inicial y descarta el `+build`. Una prerelease es menor que la
 * versión estable con el mismo núcleo: 1.0.5-alpha.1 < 1.0.5.
 */
data class Version(val core: List<Int>, val pre: List<String>) : Comparable<Version> {

    override fun compareTo(other: Version): Int {
        for (i in 0 until maxOf(core.size, other.core.size)) {
            val c = core.getOrElse(i) { 0 }.compareTo(other.core.getOrElse(i) { 0 })
            if (c != 0) return c
        }
        return when {
            pre.isEmpty() && other.pre.isEmpty() -> 0
            pre.isEmpty() -> 1
            other.pre.isEmpty() -> -1
            else -> comparePre(other.pre)
        }
    }

    private fun comparePre(other: List<String>): Int {
        for (i in 0 until maxOf(pre.size, other.size)) {
            val a = pre.getOrNull(i) ?: return -1   // el conjunto más corto es menor
            val b = other.getOrNull(i) ?: return 1
            val c = comparePreId(a, b)
            if (c != 0) return c
        }
        return 0
    }

    companion object {
        private val IDENTIFIER = Regex("[0-9A-Za-z-]+")

        /** Devuelve null si [raw] no es una versión semver válida. */
        fun parse(raw: String): Version? {
            val s = raw.trim().removePrefix("v").substringBefore('+')
            val core = s.substringBefore('-').split('.').map { it.toIntOrNull() ?: return null }
            val pre = if ('-' in s) s.substringAfter('-').split('.') else emptyList()
            if (pre.any { !IDENTIFIER.matches(it) }) return null
            return Version(core, pre)
        }

        // Identificadores numéricos: por valor y menores que los alfanuméricos.
        private fun comparePreId(a: String, b: String): Int {
            val aNum = a.all { it in '0'..'9' }
            val bNum = b.all { it in '0'..'9' }
            return when {
                aNum && bNum -> BigInteger(a).compareTo(BigInteger(b))
                aNum -> -1
                bNum -> 1
                else -> a.compareTo(b)
            }
        }
    }
}

/** True si [candidate] es estrictamente más nueva que [current]. Lo no parseable nunca es más nuevo. */
fun isNewerVersion(candidate: String, current: String): Boolean {
    val c = Version.parse(candidate) ?: return false
    val cur = Version.parse(current) ?: return false
    return c > cur
}
