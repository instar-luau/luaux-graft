const TARGETS = [x86_64-unknown-linux-gnu x86_64-pc-windows-msvc aarch64-apple-darwin]

def main []: nothing -> nothing {
    help main | print
}

def version [tag: string]: nothing -> nothing {
    let version = open Cargo.toml | get package.version

    if $tag != $"v($version)" {
        error make $"Release tag must be v($version)"
    }
}

def "main build" [
    target: string # Rust target triple to build.
    destination: path # Directory for the release archive.
]: nothing -> nothing {
    version $env.TAG

    if $target not-in $TARGETS {
        error make $"Unsupported release target: ($target)"
    }

    cargo build --release --locked --target $target

    let windows = $target ends-with windows-msvc

    let source_binary = if $windows { "luaux-graft.exe" } else { "luaux-graft" }

    let binary = if $windows { "graft.exe" } else { "graft" }

    let build_directory = $"target/($target)/release"
    let destination = $destination | path expand
    let staging = $destination | path join $"staging-($target)"
    let archive = $destination | path join $"luaux-($target).zip"

    mkdir $destination
    mkdir $staging

    cp graft.toml ($staging | path join graft.toml)
    cp --preserve [mode] ($build_directory | path join $source_binary) ($staging | path join $binary)

    try {
        cd $staging

        if $windows {
            let command = $"Compress-Archive -Path graft.toml,($binary) -DestinationPath '($archive)' -Force"
            pwsh -NoProfile -Command $command
        } else {
            ^zip -q -r $archive graft.toml $binary
        }
    } catch {|error| error make $error }

    rm --recursive --force $staging
}

def "main publish" [
    tag: string # Existing version tag to release.
    directory: path # Directory containing all target archives.
]: nothing -> nothing {
    version $tag

    let directory = $directory | path expand

    let archives = $TARGETS | each {|target|
        $directory | path join $"luaux-($target).zip"
    }

    let checksums = $archives | each {|archive|
        let checksum = open --raw $archive | hash sha256
        $"($checksum)  ($archive | path basename)"
    } | str join "\n"

    let manifest = $directory | path join checksums.txt
    $"($checksums)\n" | save --force $manifest
    gh release create $tag ...$archives $manifest --verify-tag --generate-notes
}
