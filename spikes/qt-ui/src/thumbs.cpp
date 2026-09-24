// SPDX-License-Identifier: GPL-3.0-or-later
// The C++ this spike needs: an image provider, because a QML Image cannot take bytes from Rust
// directly. Everything else is Rust and QML.
#include <QImage>
#include <QQmlApplicationEngine>
#include <QQuickImageProvider>

extern "C" {
unsigned char *spike_thumbnail_jpeg(const unsigned char *id, size_t len, size_t *out_len);
void spike_free_bytes(unsigned char *bytes, size_t len);
}

namespace {
class ThumbProvider : public QQuickImageProvider {
public:
    ThumbProvider() : QQuickImageProvider(QQuickImageProvider::Image) {}

    // Called on Qt's image loading threads, so waiting for the thumbnail here blocks nothing visible.
    QImage requestImage(const QString &id, QSize *size, const QSize &) override {
        const QByteArray name = id.toUtf8();
        size_t length = 0;
        unsigned char *bytes = spike_thumbnail_jpeg(
            reinterpret_cast<const unsigned char *>(name.constData()), name.size(), &length);
        QImage image;
        if (bytes) {
            image = QImage::fromData(bytes, static_cast<int>(length), "JPEG");
            spike_free_bytes(bytes, length);
        }
        if (size) {
            *size = image.size();
        }
        return image;
    }
};
} // namespace

extern "C" void spike_register_thumbs(void *engine) {
    static_cast<QQmlApplicationEngine *>(engine)->addImageProvider(QStringLiteral("thumbs"),
                                                                    new ThumbProvider);
}

// Test-only: QtQuickTest's runner, so that `tests/tst_*.qml` run against the real application
// module (the plain TestCase does not run outside a runner).
#include <QtQuickTest/quicktest.h>

extern "C" int spike_quick_test(int argc, char **argv, const char *dir) {
    return quick_test_main(argc, argv, "spike", dir);
}

// Translations: cxx-qt-lib has no QTranslator, so loading a .qm is a few lines of C++ too.
#include <QCoreApplication>
#include <QTranslator>

extern "C" bool spike_install_translation(const char *path) {
    auto *translator = new QTranslator(QCoreApplication::instance());
    if (!translator->load(QString::fromUtf8(path))) {
        delete translator;
        return false;
    }
    return QCoreApplication::installTranslator(translator);
}
