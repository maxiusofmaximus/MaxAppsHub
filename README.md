# 📱 MaxApps Hub

**MaxApps Hub** es un launcher de Android diseñado para simplificar la gestión y actualización de tus aplicaciones APK alojadas en GitHub. Olvídate de descargar manualmente cada nueva versión; el Hub lo hace por ti.

![📱](app/src/main/res/drawable/ic_launcher_foreground.xml) <!-- Nota: Android Studio renderiza esto, en GitHub se verá el código o puedes subir un PNG -->

## 🚀 ¿Qué hace MaxApps Hub?

Este proyecto nació de la necesidad de mantener aplicaciones personalizadas siempre actualizadas de forma automática y fluida. Sus funciones principales incluyen:

-   **Actualizaciones Automáticas**: Conecta directamente con la API de GitHub para verificar si existe una nueva versión de tus aplicaciones instaladas (como "Ley de Gases").
-   **Gestión de Estados**: Identifica si una app está:
    -   **No instalada**: Ofrece el botón para descargar e instalar desde cero.
    -   **Actualizada**: Permite abrir la app directamente.
    -   **Desactualizada**: Muestra el botón de "Actualizar" para bajar la última release.
-   **Descarga Integrada**: Utiliza el gestor de descargas de Android para obtener los APKs de forma segura y eficiente.
-   **Instalación Fluida**: Una vez terminada la descarga, el Hub abre automáticamente el instalador del sistema.
-   **Gestor de Permisos**: Si no has autorizado la instalación de aplicaciones desconocidas, el Hub te guiará directamente a la pantalla de ajustes del sistema.

## 🛠️ Mejoras Recientes (v1.0.4)

-   **Canal alfa por app**: el interruptor «Incluir alfas» muestra también los prelanzamientos, comparados como semver (`1.0.5-alpha.2` < `1.0.5`).
-   **CLI por adb**: maneja el Hub desde el PC sin tocar la pantalla (ver «CLI» abajo).

## 🛠️ Mejoras Recientes (v1.0.2)

-   **APK Firmado**: Solucionado el error de "Paquete no válido" mediante el firmado digital del APK de release.
-   **Identidad Visual**: Nuevo icono adaptativo moderno y nombre oficial del proyecto integrado.
-   **Visibilidad en Android 11+**: Implementación de `<queries>` para poder detectar aplicaciones externas correctamente.
-   **Estabilidad de Red**: Inclusión de `User-Agent` en las peticiones a GitHub para evitar bloqueos del servidor.

## 📦 Instalación

1.  Ve a la sección de [Releases](https://github.com/maxiusofmaximus/MaxAppsHub/releases).
2.  Descarga el archivo `app-release.apk` de la versión más reciente.
3.  Ábrelo en tu dispositivo Android e instálalo.
4.  ¡Empieza a gestionar tus apps favoritas!

## ⌨️ CLI

Requisitos: `adb` y el móvil con depuración USB. Desde `cli/`:

```powershell
.\mah.ps1 list                        # estado de todas las apps
.\mah.ps1 channel com.flashcards.quimica alfa   # alfa | estable
.\mah.ps1 check [pkg]                 # consulta GitHub
.\mah.ps1 update com.opencode_os.app --confirm  # descarga e instala
.\mah.ps1 status                      # progreso de la última descarga
```

`mah.sh` es el mismo comando para bash (llama a `mah.ps1`). Variables: `ADB` (ruta a adb) y `ANDROID_SERIAL` (dispositivo).

Funciona con `adb shell am broadcast` hacia `CliReceiver`, protegido con `android.permission.DUMP` (solo adb lo posee). Cada comando escribe `cli-result.json` en el almacenamiento de la app, y el cliente espera a que cambie.

Limitaciones:
-   Android exige confirmar cada instalación. `--confirm` pulsa «Instalar»/«Actualizar» en el instalador vía `uiautomator`; sin esa opción, confírmala en el móvil.
-   `update` descarga en segundo plano. Si el sistema mata el proceso de la app antes de terminar la descarga, no se abre el instalador: repite el comando.
-   La pantalla del Hub no se refresca sola con cambios hechos por CLI; ábrela de nuevo o pulsa «Buscar».

## 🧪 Aplicaciones Gestionadas Actualmente
-   **Ley de Gases (Flashcards Química)**: [Repo de la App](https://github.com/maxiusofmaximus/APK-Android-Ley-de-Gases)
-   **Atlas OS**: [Repo de la App](https://github.com/maxiusofmaximus/atlas-os)
-   **Herdr Notify**: [Repo de la App](https://github.com/maxiusofmaximus/herdr-notify)

---
Desarrollado con ❤️ para maximizar la productividad en Android.
