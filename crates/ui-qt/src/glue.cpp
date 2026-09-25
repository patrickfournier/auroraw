// SPDX-License-Identifier: GPL-3.0-or-later
// The C++ the interface needs (D-094, spike 5): an image provider, because a QML Image cannot take
// bytes from Rust directly, and a translation loader, because cxx-qt-lib has no QTranslator.
// Everything else is Rust and QML.
#include <QCoreApplication>
#include <QImage>
#include <QQmlApplicationEngine>
#include <QQmlEngine>
#include <QQuickImageProvider>
#include <QTranslator>

extern "C" {
unsigned char *auroraw_thumbnail_jpeg(const unsigned char *id, size_t len, size_t *out_len);
void auroraw_free_bytes(unsigned char *bytes, size_t len);
}

namespace {
QTranslator *g_translator = nullptr;

class ThumbProvider : public QQuickImageProvider {
public:
    ThumbProvider() : QQuickImageProvider(QQuickImageProvider::Image) {}

    // Called on Qt's image loading threads, so waiting for the thumbnail here blocks nothing visible.
    QImage requestImage(const QString &id, QSize *size, const QSize &) override {
        const QByteArray name = id.toUtf8();
        size_t length = 0;
        unsigned char *bytes = auroraw_thumbnail_jpeg(
            reinterpret_cast<const unsigned char *>(name.constData()), name.size(), &length);
        QImage image;
        if (bytes) {
            image = QImage::fromData(bytes, static_cast<int>(length), "JPEG");
            auroraw_free_bytes(bytes, length);
        }
        if (size) {
            *size = image.size();
        }
        return image;
    }
};
} // namespace

extern "C" void auroraw_setup_engine(void *engine) {
    static_cast<QQmlApplicationEngine *>(engine)->addImageProvider(QStringLiteral("thumbs"),
                                                                    new ThumbProvider);
}

// `data` stays valid for the life of the program (it is static in the Rust binary); null removes the
// current translation. `object`, when given, is an object made by QML: its engine retranslates what is
// on screen (QQmlEngine::retranslate must be called after a translator is installed).
extern "C" void auroraw_set_translation(const unsigned char *data, size_t len, QObject *object) {
    if (g_translator) {
        QCoreApplication::removeTranslator(g_translator);
        delete g_translator;
        g_translator = nullptr;
    }
    if (data && len > 0) {
        g_translator = new QTranslator(QCoreApplication::instance());
        if (g_translator->load(data, static_cast<int>(len))) {
            QCoreApplication::installTranslator(g_translator);
        } else {
            delete g_translator;
            g_translator = nullptr;
        }
    }
    if (object) {
        if (QQmlEngine *engine = qmlEngine(object)) {
            engine->retranslate();
        }
    }
}
