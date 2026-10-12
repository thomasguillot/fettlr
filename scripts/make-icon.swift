#!/usr/bin/env swift
//
// Generates Fettlr's master app icon with CoreGraphics: no Icon Composer, no external tools.
// Deterministic and re-runnable: `swift scripts/make-icon.swift` regenerates
// assets/icon-1024.png from the constants below. Tauri's own `tauri icon` command then expands
// that single 1024x1024 PNG into every platform's icon set (see README.md).
//
// The design: a rounded rect filled with the ink of the design system, with the paper glyph
// centred on top. The glyph is read straight out of assets/fettlr-glyph.svg at run time and
// filled, so the artwork lives in exactly one place and this script cannot drift from it.
//
// Ported from natalie-swift/scripts/make-icon.swift, which explains the corner-radius ratio's
// provenance (measured against a shipped macOS app icon so the two read at a consistent visual
// size once macOS wraps the flat PNG in its own squircle frame).

import AppKit
import CoreGraphics
import Foundation

let scriptURL = URL(fileURLWithPath: #filePath)
let repoRoot = scriptURL.deletingLastPathComponent().deletingLastPathComponent()
let assetsDir = repoRoot.appendingPathComponent("assets")

/// The tile and the glyph, `--color-brand` and `--color-brand-contrast` from
/// packages/design-system/src/tokens.css in sRGB. Checked below with APCA, the same measure the
/// design system's contrast gate uses, and the script refuses to render a pair under Lc 45.
let fillHex = "0B0B0D"
let glyphHex = "FBFCFD"

struct RGB { let r: Double, g: Double, b: Double }

func rgb(fromHex hex: String) -> RGB {
    var value: UInt64 = 0
    Scanner(string: hex).scanHexInt64(&value)
    return RGB(
        r: Double((value >> 16) & 0xFF) / 255,
        g: Double((value >> 8) & 0xFF) / 255,
        b: Double(value & 0xFF) / 255)
}

/// APCA (W3 0.1.9) lightness contrast of `text` on `background`, in Lc. Mirrors
/// packages/design-system/scripts/check-contrast.mjs so the icon is held to the same bar as the UI.
func apcaContrast(text: RGB, background: RGB) -> Double {
    func luminance(_ c: RGB) -> Double {
        0.2126729 * pow(c.r, 2.4) + 0.7151522 * pow(c.g, 2.4) + 0.072175 * pow(c.b, 2.4)
    }
    func clamp(_ y: Double) -> Double { y > 0.022 ? y : y + pow(0.022 - y, 1.414) }
    let textY = clamp(luminance(text))
    let bgY = clamp(luminance(background))
    if abs(bgY - textY) < 0.0005 { return 0 }
    if bgY > textY {
        let sapc = (pow(bgY, 0.56) - pow(textY, 0.57)) * 1.14
        return sapc < 0.1 ? 0 : (sapc - 0.027) * 100
    }
    let sapc = (pow(bgY, 0.65) - pow(textY, 0.62)) * 1.14
    return sapc > -0.1 ? 0 : (sapc + 0.027) * 100
}

let fill = rgb(fromHex: fillHex)
let glyph = rgb(fromHex: glyphHex)
let glyphContrast = abs(apcaContrast(text: glyph, background: fill))

print("Glyph on tile: Lc \(String(format: "%.1f", glyphContrast))")
if glyphContrast < 45 {
    fatalError("#\(glyphHex) on #\(fillHex) is Lc \(String(format: "%.1f", glyphContrast)), below the Lc 45 the design system requires")
}

/// Corner radius as a fraction of the icon's full width. Measured by alpha-scanning
/// hosts-switchr's shipped AppIcon.appiconset PNGs along their top row (y=0), finding the x pixel
/// where alpha first reaches full opacity (255): the tangent point where the corner curve meets
/// the flat top edge. Measured tangent points: 128px→29, 256px→57, 512px→114, 1024px→229, i.e. a
/// ratio of 0.223 to 0.227 depending on size; 0.224 is used as the representative value.
let iconCornerRadiusRatio: Double = 0.224

/// The glyph's longest bounding-box side, its height, as a fraction of the icon's full width. The
/// glyph is filled rather than stroked, so its bounding box is exactly its inked area. It matches
/// what the in-app mark renders (see app/src/components/FettlrMark.tsx).
let glyphSizeRatio: Double = 0.50

/// Pulls the single `d` attribute out of assets/fettlr-glyph.svg. The file is the artwork's only
/// source of truth; this script never carries a copy of the path.
func glyphPathData() -> String {
    let url = assetsDir.appendingPathComponent("fettlr-glyph.svg")
    guard let svg = try? String(contentsOf: url, encoding: .utf8) else {
        fatalError("Cannot read \(url.path)")
    }
    guard
        let range = svg.range(of: #"\sd="([^"]+)""#, options: .regularExpression),
        let quoted = svg[range].range(of: #""[^"]+""#, options: .regularExpression)
    else {
        fatalError("No path data found in \(url.lastPathComponent)")
    }
    return String(svg[quoted].dropFirst().dropLast())
}

/// Builds a `CGPath` from an SVG path string in the file's own y-down coordinate space. Supports
/// the absolute commands the glyph uses: M, L, H, V, C and Z. Anything else is a hard error rather
/// than a silently dropped segment, because a partly-drawn icon would still render.
func parsePath(_ data: String) -> CGPath {
    var tokens: [String] = []
    var number = ""
    for character in data {
        if character.isNumber || character == "." || character == "-" {
            // A minus sign starts a new number unless it is an exponent's sign.
            if character == "-" && !number.isEmpty && !number.hasSuffix("e") {
                tokens.append(number)
                number = "-"
            } else {
                number.append(character)
            }
        } else {
            if !number.isEmpty {
                tokens.append(number)
                number = ""
            }
            if character.isLetter { tokens.append(String(character)) }
        }
    }
    if !number.isEmpty { tokens.append(number) }

    let path = CGMutablePath()
    var index = 0
    var command = ""
    var current = CGPoint.zero
    var subpathStart = CGPoint.zero

    func value() -> CGFloat {
        defer { index += 1 }
        guard index < tokens.count, let parsed = Double(tokens[index]) else {
            fatalError("Malformed number in glyph path near token \(index)")
        }
        return CGFloat(parsed)
    }

    while index < tokens.count {
        if let first = tokens[index].first, first.isLetter {
            command = tokens[index]
            index += 1
            if command == "Z" || command == "z" {
                path.closeSubpath()
                current = subpathStart
                continue
            }
        }
        switch command {
        case "M":
            current = CGPoint(x: value(), y: value())
            subpathStart = current
            path.move(to: current)
        case "L":
            current = CGPoint(x: value(), y: value())
            path.addLine(to: current)
        case "H":
            current = CGPoint(x: value(), y: current.y)
            path.addLine(to: current)
        case "V":
            current = CGPoint(x: current.x, y: value())
            path.addLine(to: current)
        case "C":
            let c1 = CGPoint(x: value(), y: value())
            let c2 = CGPoint(x: value(), y: value())
            current = CGPoint(x: value(), y: value())
            path.addCurve(to: current, control1: c1, control2: c2)
        default:
            fatalError("Unsupported SVG path command '\(command)' in the glyph")
        }
    }
    return path
}

/// The glyph's `CGPath`, scaled so its longest side is `size`, centred on `center`, and flipped
/// into CoreGraphics' y-up space.
func transformedGlyphPath(center: CGPoint, size: CGFloat) -> CGPath {
    let raw = parsePath(glyphPathData())
    let bounds = raw.boundingBoxOfPath
    let scale = size / max(bounds.width, bounds.height)
    var transform = CGAffineTransform.identity
        .translatedBy(x: center.x, y: center.y)
        .scaledBy(x: scale, y: -scale)
        .translatedBy(x: -bounds.midX, y: -bounds.midY)
    guard let path = raw.copy(using: &transform) else {
        fatalError("Cannot transform the glyph path")
    }
    return path
}

func renderIcon(pixelSize: Int) -> CGImage {
    let size = CGFloat(pixelSize)
    // Explicitly sRGB, not device RGB: the PNG encoder tags its output sRGB either way, so a device
    // space here silently converts on the way out and ships a different ink than the token asks for.
    guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else {
        fatalError("sRGB colour space unavailable")
    }
    let ctx = CGContext(
        data: nil, width: pixelSize, height: pixelSize, bitsPerComponent: 8, bytesPerRow: 0,
        space: colorSpace, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.setShouldAntialias(true)
    ctx.interpolationQuality = .high

    // The rounded rect, full-bleed to the canvas edge; see `iconCornerRadiusRatio`'s doc comment.
    let radius = size * CGFloat(iconCornerRadiusRatio)
    let shapePath = CGPath(
        roundedRect: CGRect(x: 0, y: 0, width: size, height: size), cornerWidth: radius, cornerHeight: radius,
        transform: nil)

    ctx.addPath(shapePath)
    ctx.setFillColor(
        CGColor(
            colorSpace: colorSpace,
            components: [CGFloat(fill.r), CGFloat(fill.g), CGFloat(fill.b), 1])!)
    ctx.fillPath()

    let glyphPath = transformedGlyphPath(
        center: CGPoint(x: size / 2, y: size / 2), size: size * CGFloat(glyphSizeRatio))
    ctx.addPath(glyphPath)
    ctx.setFillColor(
        CGColor(
            colorSpace: colorSpace,
            components: [CGFloat(glyph.r), CGFloat(glyph.g), CGFloat(glyph.b), 1])!)
    ctx.fillPath()

    return ctx.makeImage()!
}

func writePNG(_ image: CGImage, to url: URL) {
    let rep = NSBitmapImageRep(cgImage: image)
    guard let data = rep.representation(using: .png, properties: [:]) else {
        fatalError("Failed to encode PNG for \(url.lastPathComponent)")
    }
    try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    do {
        try data.write(to: url)
    } catch {
        fatalError("Failed to write \(url.path): \(error)")
    }
}

print("\nRendering master icon:")
let masterImage = renderIcon(pixelSize: 1024)
let masterURL = assetsDir.appendingPathComponent("icon-1024.png")
writePNG(masterImage, to: masterURL)
print("  icon-1024.png (1024×1024)")

print("\nDone. Run `pnpm --dir app tauri icon ../assets/icon-1024.png` to regenerate app/src-tauri/icons/.")
