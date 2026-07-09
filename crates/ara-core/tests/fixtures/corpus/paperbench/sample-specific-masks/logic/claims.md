# Claims

## C01: Shared masks cause suboptimal per-sample performance in VR
- **Statement**: A single shared binary mask used across all samples in visual reprogramming is not optimal for all individual samples simultaneously; different samples have different optimal mask placements, and training with a shared mask causes the loss of some individual samples to increase.
- **Status**: supported
- **Falsification criteria**: If every sample achieves equal or better classification confidence with the same mask type as with its individually-optimal mask, this claim is refuted. Equivalently, if the distribution of [final loss - initial loss] under shared-mask VR contains no positive values (all samples' loss decreases), this claim is refuted.
- **Proof**: [E01]
- **Dependencies**: none
- **Tags**: shared-mask, generalization, sample-diversity, approximation-error

## C02: SMM achieves strictly lower approximation error than shared-mask VR (theoretical)
- **Statement**: The hypothesis space of SMM strictly contains the hypothesis space of shared-mask VR: Fshr(f'P) ⊆ Fsmm(f'P). By Theorem 4.2, this implies Errapx_DT(Fshr(f'P)) ≥ Errapx_DT(Fsmm(f'P)). SMM also has lower approximation error than sample-specific patterns without shared δ (Fsp ⊆ Fsmm).
- **Status**: supported
- **Falsification criteria**: Exhibit a distribution DT and pre-trained model fP for which Fshr(f'P) ⊄ Fsmm(f'P), or show Errapx(Fshr) < Errapx(Fsmm) for some DT.
- **Proof**: [E02]
- **Dependencies**: C01
- **Tags**: approximation-error, PAC-learning, hypothesis-space, theoretical

## C03: SMM outperforms all shared-mask baselines on ResNet-18 and ResNet-50 across most datasets
- **Statement**: SMM achieves higher test accuracy than Pad, Narrow, Medium, and Full watermarking baselines for ResNet-18 on 10 out of 11 datasets (all except DTD) and for ResNet-50 on all 11 datasets. Average accuracy under SMM is higher than all baselines for both models.
- **Status**: supported
- **Falsification criteria**: If any other baseline matches or exceeds SMM in average accuracy across all 11 datasets for either ResNet model, this claim is refuted.
- **Proof**: [E03]
- **Dependencies**: C01, C02
- **Tags**: empirical, ResNet, accuracy, benchmark

## C04: SMM outperforms all shared-mask baselines on ViT-B32 across most datasets
- **Statement**: SMM achieves substantially higher test accuracy than all baselines for ViT-B32 on most datasets, with average accuracy of 72.4% vs. best baseline 65.2% (Medium). Notable gains: Flowers102 (+21.8% over best baseline), Food101 (+15.4%), SUN397 (+7.3%).
- **Status**: supported
- **Falsification criteria**: If the average accuracy of SMM is not higher than Medium/Full/Narrow/Pad for ViT-B32 across the 11 datasets, this claim is refuted.
- **Proof**: [E04]
- **Dependencies**: C01, C02
- **Tags**: empirical, ViT, accuracy, benchmark

## C05: Three-channel masks outperform single-channel masks; both δ and fmask are necessary
- **Statement**: Ablation shows (i) removing fmask (only δ) reduces accuracy on feature-rich datasets; (ii) removing δ (only fmask) reduces accuracy on large-data datasets; (iii) single-channel fmask^s is worse than three-channel especially on GTSRB and Flowers102. The combination of shared δ and three-channel fmask achieves the best average performance.
- **Status**: supported
- **Falsification criteria**: If single-channel SMM achieves equal or better average accuracy than three-channel SMM across the 11 datasets, this claim is refuted.
- **Proof**: [E05]
- **Dependencies**: C02
- **Tags**: ablation, multi-channel, shared-pattern, sample-specific

## C06: Patch-wise interpolation is more computationally efficient than bilinear or bicubic interpolation
- **Statement**: Patch-wise interpolation requires 0.151×10^6 pixel accesses (vs 0.602×10^6 bilinear, 2.408×10^6 bicubic) and 0.026±0.004 s/batch (vs 0.062±0.001 bilinear, 0.195±0.013 bicubic) for ResNet-18/50. It does not require backpropagation through the interpolation step.
- **Status**: supported
- **Falsification criteria**: If bilinear or bicubic interpolation achieves fewer pixel accesses or lower batch time than patch-wise interpolation in the same experimental setup, this claim is refuted.
- **Proof**: [E06]
- **Dependencies**: none
- **Tags**: efficiency, interpolation, patch-wise, gradient
