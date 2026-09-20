#include "viewport.h"
#include <QImage>
#include <QQuickWindow>

ViewportItem::ViewportItem(QQuickItem *parent) : QQuickItem(parent) { setFlag(ItemHasContents, true); }

void ViewportItem::setFrames(const uchar *data, int w, int h, int count) {
    m_data = data; m_w = w; m_h = h; m_count = count;
}

void ViewportItem::setFrame(int index) {
    if (m_count == 0) return;
    m_index = index % m_count;
    update();
}

QSGNode *ViewportItem::updatePaintNode(QSGNode *old, UpdatePaintNodeData *) {
    if (!m_data || !window()) return old;
    auto *node = static_cast<QSGSimpleTextureNode *>(old);
    if (!node) {
        node = new QSGSimpleTextureNode;
        node->setOwnsTexture(true);
        node->setFiltering(QSGTexture::Nearest);
    }
    // A new texture per frame, as a real viewport receiving new pixels would do.
    const uchar *px = m_data + (qsizetype)m_index * m_w * m_h * 4;
    QImage img(px, m_w, m_h, m_w * 4, QImage::Format_RGBA8888);
    node->setTexture(window()->createTextureFromImage(img, QQuickWindow::TextureIsOpaque));
    node->setRect(0, 0, m_w, m_h);
    return node;
}
