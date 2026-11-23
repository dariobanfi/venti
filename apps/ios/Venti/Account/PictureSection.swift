import PhotosUI
import SwiftUI
import UIKit
import VentiCore

/// The pilot's picture on the account's page: round, as Venti serves it,
/// or the initials while there is none; "Change picture", which opens the
/// photo picker and sends the photo chosen, and "Remove picture". The
/// words are the core's (`pictureWords()`). A picture taken from Google at
/// the first sign-in is the server's doing and shows here like any other.
struct PictureSection: View {
    let signedIn: AccountView

    @Environment(AccountModel.self) private var account

    @State private var chosen: PhotosPickerItem?
    @State private var isPreparing = false

    private let words = pictureWords()

    var body: some View {
        Section {
            HStack(spacing: VentiSpace.s4) {
                PilotFace(
                    name: signedIn.displayName, picturePath: signedIn.picturePath, size: VentiSpace.s8,
                    isOwnElement: true
                )
                    .overlay {
                        if isPreparing {
                            ProgressView()
                        }
                    }
                    .accessibilityElement()
                    .accessibilityLabel(signedIn.displayName)
                    .accessibilityValue(signedIn.picturePath == nil ? "Initials" : "Picture")
                    .accessibilityIdentifier("account-picture")
                VStack(alignment: .leading, spacing: VentiSpace.s2) {
                    change
                    if signedIn.picturePath != nil {
                        Button(words.remove, role: .destructive) {
                            Task { await account.removePicture() }
                        }
                        .buttonStyle(.borderless)
                        .accessibilityIdentifier("account-picture-remove")
                    }
                }
                .font(.venti(.body))
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(.vertical, VentiSpace.s1)
        } footer: {
            Text(words.publicLine)
        }
        .disabled(account.isWorking || isPreparing || signedIn.needsSignIn)
        .onChange(of: chosen) { _, item in
            guard let item else { return }
            chosen = nil
            Task { await send(item) }
        }
    }

    /// The photo picker; under a UI test with `--sample-picture` a made-up
    /// picture in its place, the system's picker being out of the test's
    /// reach.
    @ViewBuilder private var change: some View {
        if LaunchOptions.usesSamplePicture {
            Button(words.change) {
                Task { await send(PictureUpload.sample()) }
            }
            .buttonStyle(.borderless)
            .accessibilityIdentifier("account-picture-change")
        } else {
            PhotosPicker(selection: $chosen, matching: .images, preferredItemEncoding: .compatible) {
                Text(words.change)
            }
            .buttonStyle(.borderless)
            .accessibilityIdentifier("account-picture-change")
        }
    }

    private func send(_ item: PhotosPickerItem) async {
        isPreparing = true
        let data = try? await item.loadTransferable(type: Data.self)
        let jpeg = await Task.detached { data.flatMap(PictureUpload.jpeg) }.value
        isPreparing = false
        guard let jpeg else {
            account.problem = PictureUpload.unreadable
            return
        }
        await account.setPicture(jpeg, mediaType: PictureUpload.mediaType)
    }

    private func send(_ jpeg: Data?) async {
        guard let jpeg else { return }
        await account.setPicture(jpeg, mediaType: PictureUpload.mediaType)
    }
}

/// "Show me among pilots near a place": the account's own switch, with
/// the core's line under it, shown as asked until Venti answers.
struct ShownNearSection: View {
    let signedIn: AccountView

    @Environment(AccountModel.self) private var account

    @State private var switchedTo: Bool?

    private let words = pictureWords()

    private var shown: Binding<Bool> {
        Binding(
            get: { switchedTo ?? signedIn.shownNearPlaces },
            set: { isShown in
                switchedTo = isShown
                Task {
                    await account.setShownNearPlaces(isShown)
                    switchedTo = nil
                }
            }
        )
    }

    var body: some View {
        Section {
            Toggle(isOn: shown) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(words.shownNearPlaces)
                    Text(words.shownNearPlacesLine)
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                }
            }
            .accessibilityIdentifier("account-shown-near")
        }
        .disabled(account.isWorking || signedIn.needsSignIn)
    }
}

/// A photo made ready to be sent: any picture the iPhone reads (a HEIC
/// from the camera too) as a JPEG no wider or higher than 1,000 pixels,
/// which the server cuts to its middle square of 256.
enum PictureUpload {
    static let mediaType = "image/jpeg"
    /// The longest side sent, in pixels.
    static let longestSide: CGFloat = 1000
    /// Said when the photo chosen could not be read.
    static let unreadable = "This photo could not be read. Choose another one."

    /// The picture in `data` as a JPEG of at most `longestSide`; `nil`
    /// where it is no picture.
    nonisolated static func jpeg(_ data: Data) -> Data? {
        guard let image = UIImage(data: data), image.size.width > 0, image.size.height > 0 else { return nil }
        let pixels = CGSize(width: image.size.width * image.scale, height: image.size.height * image.scale)
        let factor = min(1, longestSide / max(pixels.width, pixels.height))
        let size = CGSize(width: (pixels.width * factor).rounded(), height: (pixels.height * factor).rounded())
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = true
        let scaled = UIGraphicsImageRenderer(size: size, format: format).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        return scaled.jpegData(compressionQuality: 0.85)
    }

    /// A made-up picture for the UI tests: the pack's purple with a coral
    /// sun, nobody's face.
    static func sample() -> Data? {
        let size = CGSize(width: 640, height: 480)
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let image = UIGraphicsImageRenderer(size: size, format: format).image { context in
            UIColor(VentiTheme.light.primary).setFill()
            context.fill(CGRect(origin: .zero, size: size))
            UIColor(VentiTheme.light.accent).setFill()
            context.cgContext.fillEllipse(in: CGRect(x: 220, y: 140, width: 200, height: 200))
        }
        return image.jpegData(compressionQuality: 0.85)
    }
}
