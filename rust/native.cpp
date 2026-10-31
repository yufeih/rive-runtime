#include <memory>
#include <windows.h>
#include <dxgi1_2.h>
#include <dcomp.h>
#include "rive/file.hpp"
#include "rive/artboard.hpp"
#include "rive/animation/state_machine_instance.hpp"
#include "rive/viewmodel/runtime/viewmodel_runtime.hpp"
#include "rive/audio/audio_engine.hpp"
#include "rive/renderer/render_context.hpp"
#include "rive/renderer/d3d11/render_context_d3d_impl.hpp"
#include "rive/renderer/rive_renderer.hpp"

using Microsoft::WRL::ComPtr;
using namespace rive;
#define API extern "C" __declspec(dllexport)

struct Surface {
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<IDXGISwapChain1> compositionSwapchain;
    ComPtr<IDCompositionDevice> composition;
    ComPtr<IDCompositionTarget> target;
    ComPtr<IDCompositionVisual> visual;
    std::unique_ptr<gpu::RenderContext> runtime;
    std::unique_ptr<RiveRenderer> renderer;
    rcp<gpu::RenderTargetD3D> renderTarget;
    uint32_t width = 0;
    uint32_t height = 0;
};

API HRESULT rive_window_size(HWND window, uint32_t* width, uint32_t* height) {
    RECT rect{};
    if (!GetClientRect(window, &rect)) return HRESULT_FROM_WIN32(GetLastError());
    *width = rect.right - rect.left;
    *height = rect.bottom - rect.top;
    return S_OK;
}

static HRESULT attach_window(Surface* surface, HWND window) {
    SetLastError(0);
    auto classStyle = GetClassLongPtrW(window, GCL_STYLE);
    if (!classStyle && GetLastError()) return HRESULT_FROM_WIN32(GetLastError());
    SetLastError(0);
    if (!SetClassLongPtrW(window, GCL_STYLE, classStyle & ~(CS_OWNDC | CS_CLASSDC)) && GetLastError()) return HRESULT_FROM_WIN32(GetLastError());
    SetLastError(0);
    auto style = GetWindowLongPtrW(window, GWL_EXSTYLE);
    if (!style && GetLastError()) return HRESULT_FROM_WIN32(GetLastError());
    SetLastError(0);
    auto flags = style | WS_EX_NOREDIRECTIONBITMAP | WS_EX_LAYERED;
    if (!SetWindowLongPtrW(window, GWL_EXSTYLE, flags) && GetLastError()) return HRESULT_FROM_WIN32(GetLastError());
    if ((flags & WS_EX_LAYERED) && !SetLayeredWindowAttributes(window, 0, 255, LWA_ALPHA)) return HRESULT_FROM_WIN32(GetLastError());

    ComPtr<IDXGIDevice> dxgi;
    auto hr = surface->device.As(&dxgi);
    if (FAILED(hr)) return hr;
    ComPtr<IDXGIAdapter> adapter;
    if (FAILED(hr = dxgi->GetAdapter(&adapter))) return hr;
    ComPtr<IDXGIFactory2> factory;
    if (FAILED(hr = adapter->GetParent(IID_PPV_ARGS(&factory)))) return hr;
    DXGI_SWAP_CHAIN_DESC1 desc{};
    desc.Width = surface->width;
    desc.Height = surface->height;
    desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.BufferCount = 2;
    desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
    desc.Scaling = DXGI_SCALING_STRETCH;
    desc.AlphaMode = DXGI_ALPHA_MODE_PREMULTIPLIED;
    ComPtr<IDXGISwapChain1> swapchain;
    if (FAILED(hr = factory->CreateSwapChainForComposition(surface->device.Get(), &desc, nullptr, &swapchain))) return hr;
    ComPtr<IDCompositionDevice> composition;
    if (FAILED(hr = DCompositionCreateDevice(dxgi.Get(), IID_PPV_ARGS(&composition)))) return hr;
    ComPtr<IDCompositionTarget> target;
    if (FAILED(hr = composition->CreateTargetForHwnd(window, TRUE, &target))) return hr;
    ComPtr<IDCompositionVisual> visual;
    if (FAILED(hr = composition->CreateVisual(&visual))) return hr;
    if (FAILED(hr = visual->SetContent(swapchain.Get()))) return hr;
    if (FAILED(hr = target->SetRoot(visual.Get()))) return hr;
    if (FAILED(hr = composition->Commit())) return hr;
    surface->compositionSwapchain = std::move(swapchain);
    surface->composition = std::move(composition);
    surface->target = std::move(target);
    surface->visual = std::move(visual);
    return S_OK;
}


API HRESULT rive_surface_resize(Surface* surface, uint32_t width, uint32_t height) {
    surface->renderTarget = nullptr;
    surface->context->ClearState();
    if (surface->compositionSwapchain) {
        auto hr = surface->compositionSwapchain->ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, 0);
        if (FAILED(hr)) return hr;
    }
    surface->width = width;
    surface->height = height;
    return S_OK;
}

API HRESULT rive_surface_create(HWND window, uint32_t width, uint32_t height, Surface** output) {
    auto surface = std::make_unique<Surface>();
    const D3D_FEATURE_LEVEL levels[] = { D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0 };
    auto hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        levels, ARRAYSIZE(levels), D3D11_SDK_VERSION, &surface->device, nullptr, &surface->context);
    if (FAILED(hr)) return hr;
    ComPtr<IDXGIDevice> dxgi;
    if (FAILED(hr = surface->device.As(&dxgi))) return hr;
    ComPtr<IDXGIAdapter> adapter;
    if (FAILED(hr = dxgi->GetAdapter(&adapter))) return hr;
    DXGI_ADAPTER_DESC info{};
    if (FAILED(hr = adapter->GetDesc(&info))) return hr;
    gpu::D3DContextOptions options{};
    options.isIntel = info.VendorId == 0x8086;
    surface->runtime = gpu::RenderContextD3DImpl::MakeContext(surface->device, surface->context, options);
    if (!surface->runtime) return E_UNEXPECTED;
    surface->renderer = std::make_unique<RiveRenderer>(surface->runtime.get());
    surface->width = width;
    surface->height = height;
    if (window && FAILED(hr = attach_window(surface.get(), window))) return hr;
    *output = surface.release();
    return S_OK;
}

API void rive_surface_destroy(Surface* surface) { delete surface; }
API Factory* rive_surface_factory(Surface* surface) { return surface->runtime.get(); }
API Renderer* rive_surface_begin(Surface* surface) {
    gpu::RenderContext::FrameDescriptor frame{};
    frame.renderTargetWidth = surface->width;
    frame.renderTargetHeight = surface->height;
    frame.clearColor = 0;
    surface->runtime->beginFrame(frame);
    return surface->renderer.get();
}
API HRESULT rive_surface_present(Surface* surface, float) {
    ComPtr<ID3D11Texture2D> buffer;
    auto hr = surface->compositionSwapchain->GetBuffer(0, IID_PPV_ARGS(&buffer));
    if (FAILED(hr)) return hr;
    if (!surface->renderTarget) {
        surface->renderTarget = surface->runtime->static_impl_cast<gpu::RenderContextD3DImpl>()->makeRenderTarget(surface->width, surface->height);
    }
    surface->renderTarget->setTargetTexture(buffer);
    gpu::RenderContext::FlushResources resources{};
    resources.renderTarget = surface->renderTarget.get();
    surface->runtime->flush(resources);
    return surface->compositionSwapchain->Present(1, 0);
}

API HRESULT rive_surface_read_rgba(Surface* surface, uint8_t* pixels, size_t length) {
    if (!pixels || length != size_t(surface->width) * surface->height * 4) return E_INVALIDARG;
    D3D11_TEXTURE2D_DESC desc{};
    desc.Width = surface->width;
    desc.Height = surface->height;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_UNORDERED_ACCESS;
    ComPtr<ID3D11Texture2D> texture;
    auto hr = surface->device->CreateTexture2D(&desc, nullptr, &texture);
    if (FAILED(hr)) return hr;
    auto target = surface->runtime->static_impl_cast<gpu::RenderContextD3DImpl>()->makeRenderTarget(surface->width, surface->height);
    target->setTargetTexture(texture);
    gpu::RenderContext::FlushResources resources{};
    resources.renderTarget = target.get();
    surface->runtime->flush(resources);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    ComPtr<ID3D11Texture2D> staging;
    if (FAILED(hr = surface->device->CreateTexture2D(&desc, nullptr, &staging))) return hr;
    surface->context->CopyResource(staging.Get(), texture.Get());
    D3D11_MAPPED_SUBRESOURCE mapped{};
    if (FAILED(hr = surface->context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped))) return hr;
    for (uint32_t y = 0; y < surface->height; ++y) {
        memcpy(pixels + size_t(y) * surface->width * 4,
            static_cast<const uint8_t*>(mapped.pData) + size_t(y) * mapped.RowPitch, surface->width * 4);
    }
    surface->context->Unmap(staging.Get(), 0);
    return S_OK;
}

API HRESULT rive_audio_set_muted(bool muted) {
    return static_cast<HRESULT>(AudioEngine::SetRuntimeEnabled(!muted));
}
API File* loadRiveFile(const uint8_t* bytes, uint64_t size, Factory* factory, void*, void*) {
    return File::import(Span<const uint8_t>(bytes, size), factory).release();
}
API ArtboardInstance* riveFileArtboardDefault(File* file, bool) { return file->artboardDefault().release(); }
API StateMachineInstance* riveArtboardStateMachineDefault(ArtboardInstance* artboard) { return artboard->defaultStateMachine().release(); }
API ViewModelRuntime* riveFileDefaultArtboardViewModelRuntime(File* file, Artboard* artboard) { return ref_rcp(file->defaultArtboardViewModel(artboard)).release(); }
API ViewModelInstanceRuntime* createDefaultVMIRuntime(ViewModelRuntime* model) { return model->createDefaultInstance().release(); }
API void stateMachineSetVMIRuntime(StateMachineInstance* machine, ViewModelInstanceRuntime* instance) { machine->bindViewModelInstance(instance->instance()); }
API ViewModelInstanceNumberRuntime* vmiRuntimeGetNumberProperty(ViewModelInstanceRuntime* instance, const char* name) { return instance ? instance->propertyNumber(name) : nullptr; }
API ViewModelInstanceBooleanRuntime* vmiRuntimeGetBooleanProperty(ViewModelInstanceRuntime* instance, const char* name) { return instance ? instance->propertyBoolean(name) : nullptr; }
API ViewModelInstanceStringRuntime* vmiRuntimeGetStringProperty(ViewModelInstanceRuntime* instance, const char* name) { return instance ? instance->propertyString(name) : nullptr; }
API ViewModelInstanceListRuntime* vmiRuntimeGetListProperty(ViewModelInstanceRuntime* instance, const char* name) { return instance ? instance->propertyList(name) : nullptr; }
API size_t getVMIListRuntimeSize(ViewModelInstanceListRuntime* list) { return list->size(); }
API ViewModelInstanceRuntime* vmiListRuntimeInstanceAt(ViewModelInstanceListRuntime* list, int index) { return list->instanceAt(index).release(); }
API void setVMINumberRuntimeValue(ViewModelInstanceNumberRuntime* value, float number) { value->value(number); }
API float getVMINumberRuntimeValue(ViewModelInstanceNumberRuntime* value) { return value->value(); }
API void setVMIBooleanRuntimeValue(ViewModelInstanceBooleanRuntime* value, bool boolean) { value->value(boolean); }
API void setVMIStringRuntimeValue(ViewModelInstanceStringRuntime* value, const char* string) { value->value(string); }
API bool stateMachineInstanceAdvanceAndApply(StateMachineInstance* machine, float seconds) { return machine->advanceAndApply(seconds); }
API float riveArtboardGetOriginalWidth(Artboard* artboard) { return artboard->originalWidth(); }
API float riveArtboardGetOriginalHeight(Artboard* artboard) { return artboard->originalHeight(); }
API void save(Renderer* renderer) { renderer->save(); }
API void restore(Renderer* renderer) { renderer->restore(); }
API void transform(Renderer* renderer, float xx, float xy, float yx, float yy, float tx, float ty) { renderer->transform(Mat2D(xx, xy, yx, yy, tx, ty)); }
API void artboardDraw(Artboard* artboard, Renderer* renderer) { artboard->draw(renderer); }
API void deleteVMIValueRuntime(ViewModelInstanceValueRuntime*) {}
API void deleteVMIRuntime(ViewModelInstanceRuntime* instance) { instance->unref(); }
API void deleteViewModelRuntime(ViewModelRuntime* model) { model->unref(); }
API void deleteStateMachineInstance(StateMachineInstance* machine) { delete machine; }
API void deleteArtboardInstance(ArtboardInstance* artboard) { delete artboard; }
API void deleteRiveFile(File* file) { file->unref(); }
