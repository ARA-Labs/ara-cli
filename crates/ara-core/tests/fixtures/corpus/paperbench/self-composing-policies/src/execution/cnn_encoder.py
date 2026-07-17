"""
CNN Encoder for ALE Visual Control Tasks
Used in CompoNet for SpaceInvaders and Freeway sequences.

Architecture (Appendix E.1):
  - Conv layer 1: 32 channels, filter 8x8
  - Conv layer 2: 64 channels, filter 4x4
  - Conv layer 3: 64 channels, filter 3x3
  - Dense layer:  output dim 512

Input:  RGB image of shape (batch, C, H, W) where H=210, W=160, C=3 (or grayscale stack)
Output: Feature vector of shape (batch, 512)

Note: Encoder for new CompoNet module is initialized with weights of prior module's encoder.
"""
import torch
import torch.nn as nn
from typing import Tuple


class CNNEncoder(nn.Module):
    """
    CNN encoder for encoding ALE pixel observations to feature vectors.
    Follows CleanRL (Huang et al., 2022) implementation adapted for CompoNet.

    Input:  Pixel observations, shape (batch, channels, height, width)
            For ALE: (batch, 4, 84, 84) with grayscale frame stacking (standard preprocessing)
            Or raw: (batch, 3, 210, 160) RGB frames
    Output: Feature vector h_s of shape (batch, 512) — d_enc = 512
    """

    def __init__(self, in_channels: int = 4):
        """
        Args:
            in_channels: Number of input channels (e.g., 4 for stacked grayscale frames,
                        or 3 for RGB single frame)
        """
        super().__init__()
        self.encoder = nn.Sequential(
            # Conv layer 1: 32 channels, filter size 8, stride 4 (standard ALE preprocessing)
            nn.Conv2d(in_channels, 32, kernel_size=8, stride=4),
            nn.ReLU(),
            # Conv layer 2: 64 channels, filter size 4, stride 2
            nn.Conv2d(32, 64, kernel_size=4, stride=2),
            nn.ReLU(),
            # Conv layer 3: 64 channels, filter size 3, stride 1
            nn.Conv2d(64, 64, kernel_size=3, stride=1),
            nn.ReLU(),
            nn.Flatten(),
        )
        # Compute flattened size dynamically; for 84x84 input: 64 * 7 * 7 = 3136
        # Dense layer: output dimension 512 (d_enc)
        conv_output_size = self._get_conv_output_size(in_channels)
        self.fc = nn.Sequential(
            nn.Linear(conv_output_size, 512),
            nn.ReLU(),
        )

    def _get_conv_output_size(self, in_channels: int) -> int:
        """Compute CNN output size for a standard 84x84 preprocessed input."""
        # Standard ALE preprocessing uses 84x84 grayscale
        dummy = torch.zeros(1, in_channels, 84, 84)
        out = self.encoder(dummy)
        return out.shape[1]

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        """
        Args:
            x: Pixel observations, shape (batch, in_channels, H, W)
               Values should be normalized to [0, 1] (divide by 255)
        Returns:
            h_s: Feature vector, shape (batch, 512)
        """
        features = self.encoder(x)
        h_s = self.fc(features)
        return h_s  # shape: (batch, 512)

    def init_from_encoder(self, other_encoder: "CNNEncoder") -> None:
        """
        Initialize this encoder's weights from another encoder (prior module).
        Used when creating a new CompoNet module for a new task (Appendix E.2).

        Args:
            other_encoder: Previously trained encoder to copy weights from
        """
        self.load_state_dict(other_encoder.state_dict())
