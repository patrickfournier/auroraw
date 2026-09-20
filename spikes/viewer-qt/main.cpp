// Spike 2, Qt Quick: the same image view, grid, tree, text field and language switch as the Slint
// and iced prototypes, driven by the same pre-rendered frames. Throwaway code.
//
// usage: viewer-qt --bench view|grid|both|exact|tree [--view WxH] [--secs N] [--out result.json]
//                  [--frames dir] [--items N] [--dump-query]
#include <QFile>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>
#include <QQuickWindow>
#include <QStandardItemModel>
#include <QTranslator>
#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <mutex>
#include <thread>
#include <unistd.h>
#include <vector>
#include "viewport.h"

using Clock = std::chrono::steady_clock;
static double msOf(Clock::duration d) { return std::chrono::duration<double, std::milli>(d).count(); }
static double pct(std::vector<double> v, double q) {
    if (v.empty()) return 0;
    std::sort(v.begin(), v.end());
    return v[std::min<size_t>(v.size() - 1, size_t(v.size() * q))];
}
static double rssMb() {
    QFile f("/proc/self/status");
    if (!f.open(QIODevice::ReadOnly)) return 0;
    for (const QByteArray &l : f.readAll().split('\n'))
        if (l.startsWith("VmRSS:")) return l.simplified().split(' ')[1].toDouble() / 1024.0;
    return 0;
}

// 256 distinct 160x120 thumbnails, the same procedural pattern as the other prototypes.
class ThumbProvider : public QQuickImageProvider {
public:
    ThumbProvider() : QQuickImageProvider(QQuickImageProvider::Image) {
        for (int t = 0; t < 256; ++t) {
            QImage img(160, 120, QImage::Format_RGBA8888);
            float hue = t / 256.0f * 6.0f;
            float r = std::sin(hue) * 0.5f + 0.5f, g = std::sin(hue + 2) * 0.5f + 0.5f, b = std::sin(hue + 4) * 0.5f + 0.5f;
            for (int y = 0; y < 120; ++y) {
                uchar *line = img.scanLine(y);
                for (int x = 0; x < 160; ++x) {
                    float k = 0.35f + 0.65f * ((x / 160.0f) * (1.0f - y / 120.0f)) + 0.08f * (std::sin(x * 0.4f) * std::cos(y * 0.3f));
                    line[x * 4] = uchar(r * k * 255); line[x * 4 + 1] = uchar(g * k * 255); line[x * 4 + 2] = uchar(b * k * 255); line[x * 4 + 3] = 255;
                }
            }
            m_images.push_back(img);
        }
    }
    QImage requestImage(const QString &id, QSize *size, const QSize &) override {
        const QImage &img = m_images[id.toInt() % 256];
        if (size) *size = img.size();
        return img;
    }
private:
    std::vector<QImage> m_images;
};

class Bench : public QObject {
    Q_OBJECT
public:
    QQmlApplicationEngine *engine = nullptr;
    QTranslator translator;
    bool french = false;
    Q_INVOKABLE void toggleLanguage() {
        french = !french;
        if (french) QCoreApplication::installTranslator(&translator); else QCoreApplication::removeTranslator(&translator);
        engine->retranslate();
    }
};

int main(int argc, char **argv) {
    auto t0 = Clock::now();
    QGuiApplication app(argc, argv);
    QStringList args = app.arguments();
    auto get = [&](const QString &n, const QString &def) { int i = args.indexOf(n); return (i >= 0 && i + 1 < args.size()) ? args[i + 1] : def; };
    QString modeS = get("--bench", "view");
    bool showView = modeS == "view" || modeS == "both" || modeS == "exact";
    bool showGrid = modeS == "grid" || modeS == "both" || modeS == "tree";
    int secs = get("--secs", "10").toInt();
    int vw = modeS == "both" ? 1340 : (modeS == "exact" ? 800 : 1340), vh = modeS == "both" ? 480 : (modeS == "exact" ? 600 : 964);
    if (args.contains("--view")) { auto p = get("--view", "").split('x'); vw = p[0].toInt(); vh = p[1].toInt(); }
    int items = get("--items", "100000").toInt();
    QString framesDir = get("--frames", "samples");

    std::thread([secs] { std::this_thread::sleep_for(std::chrono::seconds(secs + 40)); fprintf(stderr, "watchdog: exiting\n"); _exit(3); }).detach();

    // The frames, shared with the other prototypes.
    QFile framesFile;
    const uchar *frames = nullptr;
    int fw = 0, fh = 0, fcount = 0;
    if (showView) {
        framesFile.setFileName(QString("%1/frames-%2x%3.bin").arg(framesDir).arg(vw).arg(vh));
        if (!framesFile.open(QIODevice::ReadOnly)) { fprintf(stderr, "missing %s (make it with dumpframes)\n", qPrintable(framesFile.fileName())); return 2; }
        const uchar *map = framesFile.map(0, framesFile.size());
        const uint32_t *hd = reinterpret_cast<const uint32_t *>(map);
        fw = hd[0]; fh = hd[1]; fcount = hd[2];
        frames = map + 12;
        if (fw != vw || fh != vh) { fprintf(stderr, "frames are %dx%d, expected %dx%d\n", fw, fh, vw, vh); return 2; }
    }

    // The keyword tree: 20 x 20 x 10 nodes.
    QStandardItemModel treeModel;
    for (int a = 0; a < 20; ++a) {
        auto *na = new QStandardItem(QString("Category %1").arg(a));
        for (int b = 0; b < 20; ++b) {
            auto *nb = new QStandardItem(QString("Group %1.%2").arg(a).arg(b));
            for (int c = 0; c < 10; ++c) nb->appendRow(new QStandardItem(QString("Keyword %1.%2.%3").arg(a).arg(b).arg(c)));
            na->appendRow(nb);
        }
        treeModel.appendRow(na);
    }

    qmlRegisterType<ViewportItem>("Spike", 1, 0, "ViewportItem");
    QQmlApplicationEngine engine;
    Bench bench;
    bench.engine = &engine;
    bench.translator.load("i18n/viewer_qt_fr.qm");
    engine.addImageProvider("thumbs", new ThumbProvider);
    int winW = showGrid ? 1600 : 260 + vw, winH = showGrid ? 1000 : 36 + vh;
    QQmlContext *ctx = engine.rootContext();
    ctx->setContextProperty("winW", winW); ctx->setContextProperty("winH", winH);
    ctx->setContextProperty("vw", vw); ctx->setContextProperty("vh", vh);
    ctx->setContextProperty("showView", showView); ctx->setContextProperty("showGrid", showGrid);
    ctx->setContextProperty("gridItems", items);
    ctx->setContextProperty("treeModel", &treeModel);
    ctx->setContextProperty("bench", &bench);

    auto tWindow = Clock::now();
    engine.load(QUrl("qrc:/qml/Main.qml"));
    if (engine.rootObjects().isEmpty()) return 1;
    QObject *root = engine.rootObjects().first();
    auto *window = qobject_cast<QQuickWindow *>(root);
    auto *viewport = root->findChild<ViewportItem *>("viewport");
    auto *grid = root->findChild<QQuickItem *>("grid");
    if (viewport && frames) viewport->setFrames(frames, fw, fh, fcount);
    if (viewport) viewport->setFrame(0);

    // The language switch, checked through a QML function so that bindings are not cached.
    QVariant en, fr;
    QMetaObject::invokeMethod(root, "caption", Q_RETURN_ARG(QVariant, en));
    bench.toggleLanguage();
    QMetaObject::invokeMethod(root, "caption", Q_RETURN_ARG(QVariant, fr));
    bench.toggleLanguage();
    bool i18nOk = en.toString() == "Keywords" && fr.toString() == "Mots-clés";
    fprintf(stderr, "language switch: '%s' -> '%s' (%s)\n", qPrintable(en.toString()), qPrintable(fr.toString()), i18nOk ? "ok" : "FAILED");

    // Frame loop and measures. The render thread only records timestamps and counts frames; the
    // GUI thread does the per-frame update.
    std::mutex mu;
    std::vector<Clock::time_point> swaps;
    std::vector<double> renderMs, updateMs;
    Clock::time_point beforeT;
    std::atomic<int> n{0};
    std::atomic<bool> done{false};
    double firstFrameMs = -1;
    Clock::time_point started{};
    bool haveStart = false;
    QString exactJson;

    auto finish = [&]() {
        std::lock_guard<std::mutex> lk(mu);
        size_t start = std::min<size_t>(30, swaps.size() ? swaps.size() - 1 : 0);
        std::vector<double> iv;
        for (size_t i = start + 1; i < swaps.size(); ++i) iv.push_back(msOf(swaps[i] - swaps[i - 1]));
        std::vector<double> rm(renderMs.begin() + std::min<size_t>(30, renderMs.size() ? renderMs.size() - 1 : 0), renderMs.end());
        double mean = 0; for (double v : iv) mean += v; mean /= std::max<size_t>(1, iv.size());
        int over = int(std::count_if(iv.begin(), iv.end(), [](double v) { return v > 20; }));
        QJsonObject o;
        o["toolkit"] = QString("Qt Quick %1").arg(qVersion()); o["mode"] = modeS;
        o["view"] = QJsonArray{vw, vh}; o["frames_measured"] = int(iv.size());
        o["frame_interval_ms"] = QJsonObject{{"mean", mean}, {"median", pct(iv, .5)}, {"p95", pct(iv, .95)}, {"p99", pct(iv, .99)}, {"max", pct(iv, 1)}};
        o["fps"] = 1000.0 / std::max(mean, 1e-9); o["frames_over_20ms"] = over;
        o["render_ms"] = QJsonObject{{"median", pct(rm, .5)}, {"p95", pct(rm, .95)}, {"p99", pct(rm, .99)}};
        o["update_ms"] = QJsonObject{{"median", pct(updateMs, .5)}, {"p95", pct(updateMs, .95)}};
        o["first_frame_ms"] = firstFrameMs; o["rss_mb"] = rssMb(); o["i18n_ok"] = i18nOk;
        o["render_loop"] = qEnvironmentVariable("QSG_RENDER_LOOP", "default (threaded)");
        o["graphics_api"] = QString::number(int(QSGRendererInterface::OpenGL));
        if (!exactJson.isEmpty()) o["pixel_exact"] = QJsonDocument::fromJson(exactJson.toUtf8()).object();
        QByteArray text = QJsonDocument(o).toJson();
        printf("%s\n", text.constData());
        QString out = get("--out", "");
        if (!out.isEmpty()) { QFile f(out); if (f.open(QIODevice::WriteOnly)) f.write(text); }
        if (args.contains("--dump-query")) { auto *sf = root->findChild<QObject *>("search"); fprintf(stderr, "query: \"%s\"\n", sf ? qPrintable(sf->property("text").toString()) : "?"); }
    };

    QObject::connect(window, &QQuickWindow::beforeRendering, window, [&] { beforeT = Clock::now(); }, Qt::DirectConnection);
    QObject::connect(window, &QQuickWindow::afterRendering, window, [&] { std::lock_guard<std::mutex> lk(mu); renderMs.push_back(msOf(Clock::now() - beforeT)); }, Qt::DirectConnection);
    QObject::connect(window, &QQuickWindow::frameSwapped, window, [&] {
        if (done) return;
        auto now = Clock::now();
        { std::lock_guard<std::mutex> lk(mu); swaps.push_back(now); }
        int k = n.fetch_add(1);
        if (k == 0) firstFrameMs = msOf(now - tWindow);
        QMetaObject::invokeMethod(window, [&, k, now] {
            if (done) return;
            if (k == 30) { started = now; haveStart = true; }
            bool finished = (modeS == "exact" && k >= 45) || (haveStart && msOf(now - started) > secs * 1000.0);
            auto t = Clock::now();
            if (showView && modeS != "exact" && viewport) viewport->setFrame(k);
            if (showGrid && modeS != "tree" && grid) {
                double total = (items / 8.0) * 124.0;  // GridView of 8 columns at this width
                grid->setProperty("contentY", std::fmod(k * 45.0, std::max(1.0, total - 600.0)));
            }
            if (modeS == "tree" && grid) grid->setProperty("contentY", std::fmod(k * 45.0, 100000.0));
            { std::lock_guard<std::mutex> lk(mu); updateMs.push_back(msOf(Clock::now() - t)); }
            if (modeS == "exact" && k == 40 && window) {
                QImage shot = window->grabWindow().convertToFormat(QImage::Format_RGBA8888);
                long bad = 0, worst = 0, total = 0;
                for (int y = 0; y < vh && y + 36 < shot.height(); ++y)
                    for (int x = 0; x < vw && x + 260 < shot.width(); ++x) {
                        const uchar *s = shot.constScanLine(y + 36) + (x + 260) * 4;
                        const uchar *f = frames + ((qsizetype)y * vw + x) * 4;
                        long d = std::max({std::abs(s[0] - f[0]), std::abs(s[1] - f[1]), std::abs(s[2] - f[2])});
                        ++total; if (d) { ++bad; worst = std::max(worst, d); }
                    }
                exactJson = QString("{\"different_pixels\":%1,\"worst_channel_difference\":%2,\"total_pixels\":%3,\"snapshot\":\"%4x%5\"}").arg(bad).arg(worst).arg(total).arg(shot.width()).arg(shot.height());
            }
            if (finished) { done = true; finish(); QGuiApplication::quit(); }
            else if (viewport && modeS == "exact") viewport->update();
            else if (window) window->update();
        }, Qt::QueuedConnection);
    }, Qt::DirectConnection);

    (void)t0;
    return app.exec();
}

#include "main.moc"
