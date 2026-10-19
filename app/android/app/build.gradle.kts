plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "dev.homemaid.homemaid"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "dev.homemaid.homemaid"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        // flutter_secure_storage needs 24.
        minSdk = maxOf(24, flutter.minSdkVersion)
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    // Two apps side by side: the real one, and the demo with synthetic data
    // (build with --flavor demo --dart-define=HOMEMAID_DEMO=true).
    buildFeatures { resValues = true }
    flavorDimensions += "data"
    productFlavors {
        create("real") {
            dimension = "data"
            resValue("string", "app_name", "HomeMaid")
        }
        create("demo") {
            dimension = "data"
            applicationIdSuffix = ".demo"
            resValue("string", "app_name", "HomeMaid démo")
        }
    }

    // The release key comes from the environment (CI secrets, see .github/workflows/ci.yml).
    // Without it, the machine's debug key, so `flutter run --release` works; but an APK signed
    // with another key does not install over the previous one.
    val keystore = System.getenv("HOMEMAID_KEYSTORE")
    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("HOMEMAID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("HOMEMAID_KEY_ALIAS")
                keyPassword = System.getenv("HOMEMAID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName(if (keystore != null) "release" else "debug")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
