// Minimal ObjC stubs for AU CocoaUI factory and view classes.
// These are compiled into the dylib so that [NSBundle classNamed:] finds them.
// At runtime, methods are replaced via method_setImplementation.

#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>

__attribute__((visibility("default")))
@interface SunmaoAUCocoaViewFactoryAuto : NSObject
@end

@implementation SunmaoAUCocoaViewFactoryAuto
+ (id)uiViewForAudioUnit:(void *)au withSize:(NSSize)size { return nil; }
+ (id)uiViewForAudioUnit:(void *)au preferredSize:(NSSize)size { return nil; }
+ (unsigned int)interfaceVersion { return 0; }
+ (NSString *)description { return @"SunmaoAUFactory"; }
- (id)uiViewForAudioUnit:(void *)au withSize:(NSSize)size { return nil; }
- (id)uiViewForAudioUnit:(void *)au preferredSize:(NSSize)size { return nil; }
- (unsigned int)interfaceVersion { return 0; }
- (NSString *)description { return @"SunmaoAUFactory"; }
@end

// View class with ivars matching what the Rust code expects.
// Method implementations are replaced at runtime.
__attribute__((visibility("default")))
@interface SunmaoAUCocoaViewAuto : NSView
{
    void *au_unit;
    void *au_instance;
    void *au_user_data;
    const void *au_superclass;
    const void *au_callbacks;
    unsigned char au_is_opengl;
    id au_timer;
    NSSize au_preferred_size;
}
@end

@implementation SunmaoAUCocoaViewAuto
@end
