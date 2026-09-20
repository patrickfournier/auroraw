#pragma once
#include <QQuickItem>
#include <QSGSimpleTextureNode>

// The image view: shows one RGBA8 frame at a time, 1:1, as a scene graph texture.
class ViewportItem : public QQuickItem {
    Q_OBJECT
public:
    explicit ViewportItem(QQuickItem *parent = nullptr);
    void setFrames(const uchar *data, int w, int h, int count);
    void setFrame(int index);

protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;

private:
    const uchar *m_data = nullptr;
    int m_w = 0, m_h = 0, m_count = 0, m_index = 0;
};
