import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';

/// Set to "Y" by managed Relivo builds: the device only accepts control once
/// it holds an enrollment token issued by the Relivo console.
const String kOptionRelivoManagementRequired = 'management-required';

/// Unique per-device enrollment token (64 lowercase hexadecimal characters).
/// Never display its value.
const String kOptionRelivoDeviceToken = 'management-device-token';

// Translation keys. They fall back to the English text when no translation
// exists.
const String kRelivoEnrollmentTitle = 'Relivo enrollment';
const String kRelivoEnrolledLabel = 'Enrolled';
const String kRelivoNotEnrolledLabel =
    'Not enrolled: this device cannot be controlled';
const String kRelivoEnterTokenButton = 'Enter enrollment token';
const String kRelivoEnrollmentHelp =
    'Signing in with an account on this PC does not enroll it. Create the device in the Relivo console with the ID above, copy the token it shows, paste it here, then approve the device in the console.';
const String kRelivoNotEnrolledHomeTip =
    'This computer cannot be controlled remotely until it is enrolled: enter the token created in the Relivo console. Not needed on a computer used only to control others.';

final RegExp _kRelivoTokenPattern = RegExp(r'^[0-9a-f]{64}$');

/// `mainGetOptionSync` reads the options cached in this process
/// (`ui_interface::OPTIONS`). `mainSetOption` updates that cache before it
/// forwards the options to the server process over IPC, and the cache is
/// refreshed from the server every second, so a synchronous read is current.
bool isRelivoManagementRequired() =>
    bind.mainGetOptionSync(key: kOptionRelivoManagementRequired) == 'Y';

bool isRelivoDeviceEnrolled() =>
    bind.mainGetOptionSync(key: kOptionRelivoDeviceToken).isNotEmpty;

/// Shared enrollment state, observed by the home page banner and the settings
/// section.
final RxBool relivoDeviceEnrolled = isRelivoDeviceEnrolled().obs;

/// Re-reads the token option. Do not call during a build.
void refreshRelivoEnrollmentState() {
  relivoDeviceEnrolled.value = isRelivoDeviceEnrolled();
}

/// Returns the normalized token, or null if [input] is not 64 hexadecimal
/// characters. Upper-case input is accepted and lower-cased.
String? normalizeRelivoEnrollmentToken(String input) {
  final token = input.trim().toLowerCase();
  return _kRelivoTokenPattern.hasMatch(token) ? token : null;
}

void showRelivoEnrollmentDialog() {
  final controller = TextEditingController();
  var errorMsg = '';
  var isInProgress = false;

  gFFI.dialogManager.show((setState, close, context) {
    submit() async {
      if (isInProgress) {
        return;
      }
      final token = normalizeRelivoEnrollmentToken(controller.text);
      if (token == null) {
        setState(() {
          errorMsg =
              'Invalid token: it must be exactly 64 hexadecimal characters (0-9, a-f), as shown in the Relivo console.';
        });
        return;
      }
      setState(() {
        errorMsg = '';
        isInProgress = true;
      });
      try {
        await bind.mainSetOption(key: kOptionRelivoDeviceToken, value: token);
      } catch (e) {
        debugPrint('Failed to save the Relivo enrollment token: $e');
        setState(() {
          isInProgress = false;
          errorMsg = 'The enrollment token could not be saved.';
        });
        return;
      }
      refreshRelivoEnrollmentState();
      close();
      _showRelivoEnrollmentSavedDialog();
    }

    return CustomAlertDialog(
      title: Text(translate(kRelivoEnterTokenButton)),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(translate(kRelivoEnrollmentHelp)),
          const SizedBox(height: 12.0),
          TextField(
            controller: controller,
            autofocus: true,
            autocorrect: false,
            enableSuggestions: false,
            maxLines: 1,
            enabled: !isInProgress,
            decoration: InputDecoration(
              labelText: translate('Enrollment token'),
              hintText: translate('64 hexadecimal characters'),
              errorText: errorMsg.isEmpty ? null : translate(errorMsg),
              errorMaxLines: 3,
            ),
            onChanged: (_) {
              if (errorMsg.isNotEmpty) {
                setState(() {
                  errorMsg = '';
                });
              }
            },
            onSubmitted: (_) => submit(),
          ).workaroundFreezeLinuxMint(),
          // NOT use Offstage to wrap LinearProgressIndicator
          if (isInProgress) const LinearProgressIndicator().marginOnly(top: 8),
        ],
      ),
      actions: [
        dialogButton('Cancel', onPressed: close, isOutline: true),
        dialogButton('Save', onPressed: isInProgress ? null : submit),
      ],
      onSubmit: submit,
      onCancel: close,
    );
  });
}

void _showRelivoEnrollmentSavedDialog() {
  gFFI.dialogManager.show((setState, close, context) {
    return CustomAlertDialog(
      title: Text(translate('Enrollment token saved')),
      content: Text(translate(
          'In the Relivo console, check that the device ID matches this device, then approve the device. It can be controlled only after approval.')),
      actions: [
        dialogButton('OK', onPressed: close),
      ],
      onSubmit: close,
      onCancel: close,
    );
  });
}
