import QtQuick
import QtQuick.Shapes
import qs.Commons

Item {
  id: root

  property real iconSize: Style.font.icon
  property color color: Color.foreground

  width: iconSize
  height: iconSize
  implicitWidth: iconSize
  implicitHeight: iconSize

  Shape {
    anchors.fill: parent
    antialiasing: true
    layer.enabled: true
    layer.samples: 4

    ShapePath {
      fillColor: root.color
      strokeColor: root.color
      strokeWidth: root.iconSize * 0.06
      startX: root.width * 0.5
      startY: root.height * 0.06
      PathLine { x: root.width * 0.92; y: root.height * 0.22 }
      PathLine { x: root.width * 0.92; y: root.height * 0.52 }
      PathQuad { controlX: root.width * 0.92; controlY: root.height * 0.82; x: root.width * 0.5; y: root.height * 0.96 }
      PathQuad { controlX: root.width * 0.08; controlY: root.height * 0.82; x: root.width * 0.08; y: root.height * 0.52 }
      PathLine { x: root.width * 0.08; y: root.height * 0.22 }
      PathLine { x: root.width * 0.5; y: root.height * 0.06 }
    }

    ShapePath {
      fillColor: "transparent"
      strokeColor: root.color
      strokeWidth: root.iconSize * 0.09
      capStyle: ShapePath.RoundCap
      startX: root.width * 0.5
      startY: root.height * 0.38
      PathLine { x: root.width * 0.5; y: root.height * 0.58 }
    }

    ShapePath {
      fillColor: root.color
      strokeColor: root.color
      strokeWidth: root.iconSize * 0.05
      startX: root.width * 0.5
      startY: root.height * 0.66
      PathLine { x: root.width * 0.5; y: root.height * 0.7 }
    }
  }
}
