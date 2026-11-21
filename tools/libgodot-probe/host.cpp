// Linux development probe for Godot 4.6's public LibGodot/GDExtension ABI.
// The matching source checkout supplies storage sizes for opaque ABI values.
#include "core/extension/libgodot.h"
#include "core/variant/variant.h"

#include <array>
#include <chrono>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <dlfcn.h>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <unistd.h>
#include <vector>

namespace {
constexpr uint32_t width = 256;
constexpr uint32_t height = 160;
constexpr int64_t frame_bytes = width * height * 4;

class Library {
public:
    explicit Library(const char *path) : handle(dlopen(path, RTLD_NOW | RTLD_LOCAL)) {
        if (!handle) {
            throw std::runtime_error(dlerror());
        }
    }
    ~Library() { dlclose(handle); }
    Library(const Library &) = delete;
    Library &operator=(const Library &) = delete;

    template <typename Function>
    Function symbol(const char *name) const {
        dlerror();
        auto address = dlsym(handle, name);
        if (const char *error = dlerror()) {
            throw std::runtime_error(error);
        }
        return reinterpret_cast<Function>(address);
    }

private:
    void *handle;
};

GDExtensionInterfaceGetProcAddress interface_lookup = nullptr;

template <typename Function>
Function api(const char *name) {
    auto address = interface_lookup(name);
    if (!address) {
        throw std::runtime_error(std::string("Missing GDExtension function ") + name);
    }
    return reinterpret_cast<Function>(address);
}

void on_level(void *, GDExtensionInitializationLevel) {}

GDExtensionBool initialize(GDExtensionInterfaceGetProcAddress lookup,
        GDExtensionClassLibraryPtr, GDExtensionInitialization *init) {
    interface_lookup = lookup;
    *init = {GDEXTENSION_INITIALIZATION_SCENE, nullptr, on_level, on_level};
    return true;
}

class Name {
public:
    explicit Name(const char *name) {
        api<GDExtensionInterfaceStringNameNewWithLatin1Chars>("string_name_new_with_latin1_chars")(
            storage.data(), name, false);
    }
    ~Name() {
        api<GDExtensionInterfaceVariantGetPtrDestructor>("variant_get_ptr_destructor")(
            GDEXTENSION_VARIANT_TYPE_STRING_NAME)(storage.data());
    }
    Name(const Name &) = delete;
    Name &operator=(const Name &) = delete;
    auto pointer() const { return storage.data(); }

private:
    alignas(StringName) std::array<std::byte, sizeof(StringName)> storage;
};

class Value {
public:
    Value() = default;
    ~Value() {
        if (initialized) {
            api<GDExtensionInterfaceVariantDestroy>("variant_destroy")(storage.data());
        }
    }
    Value(const Value &) = delete;
    Value &operator=(const Value &) = delete;

    void from_object(GDExtensionObjectPtr object) {
        api<GDExtensionInterfaceGetVariantFromTypeConstructor>("get_variant_from_type_constructor")(
            GDEXTENSION_VARIANT_TYPE_OBJECT)(storage.data(), &object);
        initialized = true;
    }

    void call(const char *method, Value &result) {
        Name name(method);
        GDExtensionCallError error{};
        api<GDExtensionInterfaceVariantCall>("variant_call")(
            storage.data(), name.pointer(), nullptr, 0, result.storage.data(), &error);
        result.initialized = true;
        if (error.error != GDEXTENSION_CALL_OK) {
            throw std::runtime_error(std::string("Godot call failed: ") + method
                + " (" + std::to_string(error.error) + ")");
        }
    }

    auto type() const {
        return api<GDExtensionInterfaceVariantGetType>("variant_get_type")(storage.data());
    }

    template <typename T>
    T scalar(GDExtensionVariantType expected) {
        require(expected);
        T result{};
        api<GDExtensionInterfaceGetVariantToTypeConstructor>("get_variant_to_type_constructor")(
            expected)(&result, storage.data());
        return result;
    }

    const uint8_t *frame_pixels() {
        require(GDEXTENSION_VARIANT_TYPE_PACKED_BYTE_ARRAY);
        Value length;
        call("size", length);
        if (length.scalar<int64_t>(GDEXTENSION_VARIANT_TYPE_INT) != frame_bytes) {
            throw std::runtime_error("Godot returned an unexpected RGBA frame size");
        }
        auto array = api<GDExtensionInterfaceVariantGetPtrInternalGetter>(
            "variant_get_ptr_internal_getter")(GDEXTENSION_VARIANT_TYPE_PACKED_BYTE_ARRAY)(
                storage.data());
        return api<GDExtensionInterfacePackedByteArrayOperatorIndexConst>(
            "packed_byte_array_operator_index_const")(array, 0);
    }

private:
    void require(GDExtensionVariantType expected) const {
        if (type() != expected) {
            throw std::runtime_error("Godot returned an unexpected Variant type");
        }
    }

    alignas(Variant) std::array<std::byte, sizeof(Variant)> storage;
    bool initialized = false;
};

void write_bytes(FILE *output, const void *data, size_t bytes) {
    if (std::fwrite(data, 1, bytes, output) != bytes) {
        throw std::runtime_error("Could not write RGBA frame stream");
    }
}

void write_u32(FILE *output, uint32_t value) {
    const std::array<uint8_t, 4> bytes = {uint8_t(value), uint8_t(value >> 8),
        uint8_t(value >> 16), uint8_t(value >> 24)};
    write_bytes(output, bytes.data(), bytes.size());
}

void run_probe(GDExtensionObjectPtr instance, FILE *output, bool abi_only) {
    Value host;
    host.from_object(instance);
    Value started;
    host.call("start", started);
    if (!started.scalar<GDExtensionBool>(GDEXTENSION_VARIANT_TYPE_BOOL)) {
        throw std::runtime_error("Could not start LibGodot");
    }

    Name engine_name("Engine");
    auto singleton = api<GDExtensionInterfaceGlobalGetSingleton>("global_get_singleton")(
        engine_name.pointer());
    if (!singleton) {
        throw std::runtime_error("Godot Engine singleton unavailable");
    }
    Value engine;
    engine.from_object(singleton);
    Value tree;
    engine.call("get_main_loop", tree);
    if (abi_only) {
        Value ready;
        tree.call("fixture_ready", ready);
        if (!ready.scalar<GDExtensionBool>(GDEXTENSION_VARIANT_TYPE_BOOL)) {
            throw std::runtime_error("The Godot script did not initialize the fixture");
        }
        std::fputs("LibGodot initialized the Steelgum probe over the 4.6 ABI.\n", stderr);
        return;
    }

    write_bytes(output, "SGRGBA01", 8);
    for (uint32_t index = 0; index < 2; ++index) {
        Value request;
        tree.call("request_frame", request);
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(20);
        for (;;) {
            Value quit;
            host.call("iteration", quit);
            if (quit.scalar<GDExtensionBool>(GDEXTENSION_VARIANT_TYPE_BOOL)) {
                throw std::runtime_error("LibGodot exited before publishing a frame");
            }
            Value frame;
            tree.call("take_frame", frame);
            if (frame.type() != GDEXTENSION_VARIANT_TYPE_NIL) {
                const auto *bytes = frame.frame_pixels();
                write_u32(output, index);
                write_u32(output, width);
                write_u32(output, height);
                write_bytes(output, bytes, frame_bytes);
                if (std::fflush(output) != 0) {
                    throw std::runtime_error("Could not flush RGBA frame stream");
                }
                break;
            }
            if (std::chrono::steady_clock::now() > deadline) {
                throw std::runtime_error("Timed out waiting for the Godot SubViewport");
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(8));
        }
    }
}
} // namespace

int main(int argc, char *argv[]) {
    if (argc < 3) {
        std::fprintf(stderr, "Usage: %s LIBGODOT PROJECT_DIR [--abi-only | GODOT_FLAGS...]\n", argv[0]);
        return EXIT_FAILURE;
    }
    try {
        // Reserve stdout for frames. Godot diagnostics and our errors go to stderr.
        const int frame_fd = dup(STDOUT_FILENO);
        if (frame_fd == -1 || dup2(STDERR_FILENO, STDOUT_FILENO) == -1) {
            throw std::runtime_error("Could not separate frame output from Godot diagnostics");
        }
        auto close_stream = [](FILE *stream) { std::fclose(stream); };
        std::unique_ptr<FILE, decltype(close_stream)> output(fdopen(frame_fd, "wb"), close_stream);
        if (!output) {
            close(frame_fd);
            throw std::runtime_error("Could not open the RGBA frame stream");
        }
        Library library(argv[1]);
        auto create = library.symbol<decltype(&libgodot_create_godot_instance)>(
            "libgodot_create_godot_instance");
        auto destroy = library.symbol<decltype(&libgodot_destroy_godot_instance)>(
            "libgodot_destroy_godot_instance");
        std::array options = {argv[0], const_cast<char *>("--path"), argv[2],
            const_cast<char *>("--script"), const_cast<char *>("res://probe.gd"),
            const_cast<char *>("--audio-driver"), const_cast<char *>("Dummy")};
        std::vector<char *> arguments(options.begin(), options.end());
        const bool abi_only = argc == 4 && std::string(argv[3]) == "--abi-only";
        if (abi_only) {
            arguments.push_back(const_cast<char *>("--headless"));
        } else {
            arguments.insert(arguments.end(), argv + 3, argv + argc);
        }
        auto *instance = create(static_cast<int>(arguments.size()), arguments.data(), initialize);
        if (!instance) {
            throw std::runtime_error("Could not create a LibGodot instance");
        }
        std::unique_ptr<void, decltype(destroy)> owner(instance, destroy);
        GDExtensionGodotVersion version{};
        api<GDExtensionInterfaceGetGodotVersion>("get_godot_version")(&version);
        if (version.major != 4 || version.minor != 6) {
            throw std::runtime_error("This probe was written for the Godot 4.6 ABI");
        }
        run_probe(instance, output.get(), abi_only);
        return EXIT_SUCCESS;
    } catch (const std::exception &failure) {
        std::fprintf(stderr, "LibGodot probe: %s\n", failure.what());
        return EXIT_FAILURE;
    }
}
